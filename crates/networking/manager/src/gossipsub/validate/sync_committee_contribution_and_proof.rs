use anyhow::anyhow;
use ream_bls::{PublicKey, traits::Verifiable};
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::{
    constants::beacon::{DOMAIN_SYNC_COMMITTEE, SYNC_COMMITTEE_SIZE},
    misc::{compute_epoch_at_slot, compute_signing_root, compute_sync_committee_period},
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
    contribution_and_proof::SignedContributionAndProof,
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

    let current_slot: u64 = block.message.slot;

    // [IGNORE] if contribution.slot is equal to or earlier than the current_slot (with a
    // MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
    if contribution.slot != current_slot {
        return Ok(ValidationResult::Ignore(
            "Contribution is from a future slot".to_string(),
        ));
    }

    // [REJECT] if contribution.subcommittee_index is out of SYNC_COMMITTEE_SUBNET_COUNT range.
    if contribution.subcommittee_index >= SYNC_COMMITTEE_SUBNET_COUNT {
        return Ok(ValidationResult::Reject(
            "The subcommittee index is out of range".to_string(),
        ));
    }

    // [REJECT] if contribution doesn't have any participants
    if contribution.aggregation_bits.num_set_bits() == 0 {
        return Ok(ValidationResult::Reject(
            "The contribution has too many participants".to_string(),
        ));
    }

    // [REJECT] if is_sync_committee_aggregator(contribution_and_proof.selection_proof) is false
    if !is_sync_committee_aggregator(&contribution_and_proof.selection_proof) {
        return Ok(ValidationResult::Reject(
            "The selection proof is not a valid aggregator".to_string(),
        ));
    }

    // [REJECT] if the validator with index contribution_and_proof.aggregator_index is not in the
    // sync committee for the epoch of contribution.slot
    let validator_pubkey = &state
        .validators
        .get(usize::try_from(contribution_and_proof.aggregator_index)?)
        .ok_or_else(|| anyhow!("invalid aggregator_index"))?
        .public_key;

    let is_valid_committee_member =
        get_sync_subcommittee_pubkeys(&state, contribution.subcommittee_index)
            .contains(validator_pubkey);

    if !is_valid_committee_member {
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

    let selection_data = SyncAggregatorSelectionData {
        slot: contribution.slot,
        subcommittee_index: contribution.subcommittee_index,
    };
    let current_epoch = state.get_current_epoch();

    let is_selection_proof_valid = contribution_and_proof.selection_proof.verify(
        validator_pubkey,
        compute_signing_root(
            selection_data,
            state.get_domain(DOMAIN_SYNC_COMMITTEE_SELECTION_PROOF, Some(current_epoch)),
        )
        .as_slice(),
    )?;

    if !is_selection_proof_valid {
        return Ok(ValidationResult::Reject(
            "The selection proof is not a valid signature".to_string(),
        ));
    }

    // [REJECT] if aggregate signature is not valid for the message beacon_block_root and aggregate
    // pubkey

    let sync_committee_validators =
        get_sync_subcommittee_pubkeys(&state, contribution.subcommittee_index);

    let is_sync_committee_valid = contribution.signature.fast_aggregate_verify(
        sync_committee_validators
            .iter()
            .collect::<Vec<&PublicKey>>(),
        compute_signing_root(
            contribution.beacon_block_root,
            state.get_domain(DOMAIN_SYNC_COMMITTEE, Some(current_epoch)),
        )
        .as_slice(),
    )?;

    if !is_sync_committee_valid {
        return Ok(ValidationResult::Reject(
            "The aggregate signature is not valid".to_string(),
        ));
    }

    // [REJECT] if aggregator signature of signed_contribution_and_proof.signature is not valid.
    let is_contribution_and_proof_valid = signed_contribution_and_proof.signature.verify(
        validator_pubkey,
        compute_signing_root(
            contribution_and_proof,
            state.get_domain(DOMAIN_CONTRIBUTION_AND_PROOF, Some(current_epoch)),
        )
        .as_slice(),
    )?;

    if !is_contribution_and_proof_valid {
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
