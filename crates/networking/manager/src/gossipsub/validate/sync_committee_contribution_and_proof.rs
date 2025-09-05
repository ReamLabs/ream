use alloy_primitives::FixedBytes;
use anyhow::anyhow;
use ream_bls::{BLSSignature, PublicKey, traits::Verifiable};
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::{
    constants::beacon::{DOMAIN_SYNC_COMMITTEE, SYNC_COMMITTEE_SIZE},
    misc::{
        compute_domain, compute_epoch_at_slot, compute_signing_root, compute_sync_committee_period,
    },
};
use ream_storage::{
    cache::{CacheSyncCommitteeContribution, CachedDB, SyncCommitteeKey},
    tables::table::Table,
};
use ream_validator_beacon::{
    constants::{
        DOMAIN_CONTRIBUTION_AND_PROOF, DOMAIN_SYNC_COMMITTEE_SELECTION_PROOF,
        SYNC_COMMITTEE_SUBNET_COUNT,
    },
    contribution_and_proof::{ContributionAndProof, SignedContributionAndProof},
    sync_committee::{SyncAggregatorSelectionData, is_sync_committee_aggregator},
};

use super::result::ValidationResult;

pub async fn validate_sync_committee_contribution_and_proof(
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
    signed_contribution_and_proof: &SignedContributionAndProof,
) -> anyhow::Result<ValidationResult> {
    let contribution_and_proof = &signed_contribution_and_proof.message;
    let contribution = &contribution_and_proof.contribution;

    let store = beacon_chain.store.lock().await;
    let head_root = store.get_head()?;

    let block = store
        .db
        .beacon_block_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("Could not get block for head root: {head_root}"))?;

    let state = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    let current_slot = block.message.slot;

    // [IGNORE] contribution.slot is equal to or earlier than the current_slot (with a
    // MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
    if contribution.slot > current_slot {
        return Ok(ValidationResult::Ignore(
            "Contribution is from a future slot".to_string(),
        ));
    }

    // [IGNORE] the epoch of contribution.slot is either the current or previous epoch (with a
    // MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
    let attestation_epoch = compute_epoch_at_slot(contribution.slot);
    let current_epoch = state.get_current_epoch();
    let previous_epoch = state.get_previous_epoch();

    // [IGNORE] the epoch of contribution.slot is either the current or previous epoch (with a
    // MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
    if attestation_epoch != current_epoch && attestation_epoch != previous_epoch {
        return Ok(ValidationResult::Ignore(
            "Contribution is from a epoch too far in the past".to_string(),
        ));
    }

    // [REJECT] contribution.subcommittee_index is less than SYNC_COMMITTEE_SUBNET_COUNT.
    if contribution.subcommittee_index < SYNC_COMMITTEE_SUBNET_COUNT {
        return Ok(ValidationResult::Reject(
            "The subcommittee index is out of range".to_string(),
        ));
    }

    // [REJECT] if contribution has participants
    if contribution.aggregation_bits.num_set_bits() != 0 {
        return Ok(ValidationResult::Reject(
            "The contribution has too many participants".to_string(),
        ));
    }

    // [REJECT] if is_sync_committee_aggregator(contribution_and_proof.selection_proof) is false
    if is_sync_committee_aggregator(&contribution_and_proof.selection_proof) {
        return Ok(ValidationResult::Reject(
            "The selection proof is not a valid aggregator".to_string(),
        ));
    }

    // [REJECT] if the validator with index contribution_and_proof.aggregator_index is not in the
    // sync committee for the epoch of contribution.slot
    let validator_pubkey =
        &state.validators[contribution_and_proof.aggregator_index as usize].public_key;

    let sync_committee_validators =
        get_sync_subcommittee_pubkeys(&state, contribution.subcommittee_index);

    if sync_committee_validators.contains(validator_pubkey) {
        return Ok(ValidationResult::Reject(
            "The aggregator is in the subcommittee".to_string(),
        ));
    }

    // [IGNORE] if a valid sync committee contribution with equal slot, beacon_block_root and
    // subcommittee_index has already been seen.
    let sync_contribution = CacheSyncCommitteeContribution {
        slot: contribution.slot,
        beacon_block_root: contribution.beacon_block_root,
        subcommittee_index: contribution.subcommittee_index,
    };

    if cached_db
        .seen_sync_committee_contributions
        .read()
        .await
        .contains(&sync_contribution)
    {
        return Ok(ValidationResult::Ignore(
            "A valid sync committee contribution with equal slot, beacon_block_root and subcommittee_index has already been seen".to_string(),
        ));
    }

    cached_db
        .seen_sync_committee_contributions
        .write()
        .await
        .put(sync_contribution, ());

    // [IGNORE] if a valid sync committee contribution has already been seen from the
    // aggregator with index contribution_and_proof.aggregator_index for the slot contribution.slot
    // and subcommittee index contribution.subcommittee_index.
    let sync_committee_key = SyncCommitteeKey {
        subnet_id: contribution.subcommittee_index,
        slot: contribution.slot,
        validator_index: contribution_and_proof.aggregator_index,
    };

    if cached_db
        .seen_sync_messages
        .read()
        .await
        .contains(&sync_committee_key)
    {
        return Ok(ValidationResult::Ignore(
            "A valid sync committee contribution for this aggregator, slot and subcommittee index has already been seen".to_string(),
        ));
    }

    cached_db
        .seen_sync_messages
        .write()
        .await
        .put(sync_committee_key, ());

    // [REJECT] if contribution_and_proof.selection_proof is not a valid signature of the
    // SyncAggregatorSelectionData derived from the contribution of the validator
    if !check_sync_committee_selection_data(
        state.fork.current_version,
        &SyncAggregatorSelectionData {
            slot: contribution.slot,
            subcommittee_index: contribution.subcommittee_index,
        },
        &contribution_and_proof.selection_proof,
        validator_pubkey,
    )? {
        return Ok(ValidationResult::Reject(
            "The selection proof is not a valid signature".to_string(),
        ));
    }

    // [REJECT] if aggregate signature is not valid for the message beacon_block_root and aggregate
    // pubkey
    if !check_sync_committee(
        state.fork.current_version,
        &contribution.beacon_block_root,
        &sync_committee_validators,
        &contribution.signature,
    )? {
        return Ok(ValidationResult::Reject(
            "The aggregate signature is not valid".to_string(),
        ));
    }

    // [REJECT] if aggregator signature of signed_contribution_and_proof.signature is not valid.
    if !check_contribution_and_proof(
        state.fork.current_version,
        contribution_and_proof,
        &signed_contribution_and_proof.signature,
        validator_pubkey,
    )? {
        return Ok(ValidationResult::Reject(
            "The aggregator signature is not valid".to_string(),
        ));
    }

    Ok(ValidationResult::Accept)
}

pub fn get_sync_subcommittee_pubkeys(
    state: &BeaconState,
    subcommittee_index: u64,
) -> Vec<PublicKey> {
    let current_epoch = state.get_current_epoch();

    let next_slot_epoch = compute_epoch_at_slot(state.slot + 1);
    let sync_committee = if compute_sync_committee_period(current_epoch)
        == compute_sync_committee_period(next_slot_epoch)
    {
        state.current_sync_committee.as_ref()
    } else {
        state.next_sync_committee.as_ref()
    };

    let sync_subcommittee_size = SYNC_COMMITTEE_SIZE / SYNC_COMMITTEE_SUBNET_COUNT;
    let start = (subcommittee_index * sync_subcommittee_size) as usize;

    let end = start + sync_subcommittee_size as usize;
    sync_committee.public_keys[start..end].to_vec()
}

fn check_sync_committee_selection_data(
    fork_version: FixedBytes<4>,
    selection_data: &SyncAggregatorSelectionData,
    signature: &BLSSignature,
    validator_pubkey: &PublicKey,
) -> anyhow::Result<bool> {
    let domain = compute_domain(
        DOMAIN_SYNC_COMMITTEE_SELECTION_PROOF,
        Some(fork_version),
        None,
    );

    let signing_root = compute_signing_root(selection_data, domain);

    Ok(signature.verify(validator_pubkey, signing_root.as_slice())?)
}

fn check_sync_committee(
    fork_version: FixedBytes<4>,
    beacon_block_root: &FixedBytes<32>,
    validators: &[PublicKey],
    signature: &BLSSignature,
) -> anyhow::Result<bool> {
    let domain = compute_domain(DOMAIN_SYNC_COMMITTEE, Some(fork_version), None);

    let signing_root = compute_signing_root(beacon_block_root, domain);
    Ok(signature.fast_aggregate_verify(
        validators.iter().collect::<Vec<&PublicKey>>(),
        signing_root.as_slice(),
    )?)
}

fn check_contribution_and_proof(
    fork_version: FixedBytes<4>,
    contribution_and_proof: &ContributionAndProof,
    signature: &BLSSignature,
    validator_pubkey: &PublicKey,
) -> anyhow::Result<bool> {
    let domain = compute_domain(DOMAIN_CONTRIBUTION_AND_PROOF, Some(fork_version), None);
    let signing_root = compute_signing_root(contribution_and_proof, domain);
    Ok(signature.verify(validator_pubkey, signing_root.as_slice())?)
}
