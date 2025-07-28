use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_bls::traits::Verifiable;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::{
    constants::{DOMAIN_SYNC_COMMITTEE, SYNC_COMMITTEE_SUBNET_COUNT},
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use ream_storage::{
    cache::{CachedDB, SyncCommitteeKey},
    tables::Table,
};
use ream_validator_beacon::{
    contribution_and_proof::SignedContributionAndProof,
    sync_committee::{SyncAggregatorSelectionData, is_sync_committee_aggregator},
};
use tree_hash::TreeHash;

use super::result::ValidationResult;

pub async fn validate_sync_committee_contribution_and_proof(
    contribution: &SignedContributionAndProof,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    let head_root = store.get_head()?;
    let state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    // [IGNORE] The contribution's slot is for the current slot
    if contribution.message.contribution.slot != store.get_current_slot()? {
        return Ok(ValidationResult::Ignore(
            "The proposer slashing is not the first valid".into(),
        ));
    }

    // [REJECT] The subcommittee index is in the allowed range, i.e. contribution.subcommittee_index < SYNC_COMMITTEE_SUBNET_COUNT.
    if contribution.message.contribution.subcommittee_index >= SYNC_COMMITTEE_SUBNET_COUNT as u64 {
        return Ok(ValidationResult::Reject(
            "The subcommittee index is not in the allowed range".into(),
        ));
    }

    // [REJECT] The contribution has participants.
    if contribution.message.contribution.aggregation_bits.len() == 0 {
        return Ok(ValidationResult::Reject(
            "The contribution has no participants".into(),
        ));
    }

    // [REJECT] contribution_and_proof.selection_proof selects the validator as an aggregator for the slot -- i.e. is_sync_committee_aggregator(contribution_and_proof.selection_proof) returns True.
    if !is_sync_committee_aggregator(&contribution.signature) {
        return Ok(ValidationResult::Reject(
            "The validator is not an aggregator".into(),
        ));
    }

    // [REJECT] The aggregator's validator index is in the declared subcommittee of the current sync committee.
    let aggregator_pubkey = &state
        .validators
        .get(contribution.message.aggregator_index as usize)
        .ok_or_else(|| anyhow!("Aggregator index not found in validator set"))?
        .public_key;

    if let None = state
        .get_sync_subcommittee_pubkeys(contribution.message.contribution.subcommittee_index)
        .iter()
        .find(|pubkey| pubkey == &aggregator_pubkey)
    {
        return Ok(ValidationResult::Reject(
                "The aggregator's validator index is not in the declared subcommittee of the current sync committee".into(),
            ));
    }

    //  A valid sync committee contribution with equal slot, beacon_block_root and subcommittee_index whose aggregation_bits is non-strict superset has not already been seen.
    let beacon_block_root = state.get_block_root_at_slot(contribution.message.contribution.slot)?;
    if let Some(aggregation_bits) = cached_db.seen_sync_contribution.read().await.peek(&(
        contribution.message.contribution.slot,
        beacon_block_root,
        contribution.message.contribution.subcommittee_index,
    )) {}

    // The sync committee contribution is the first valid contribution received for the aggregator with index contribution_and_proof.aggregator_index for the slot contribution.slot and subcommittee index contribution.subcommittee_index


    // [REJECT] The contribution_and_proof.selection_proof is a valid signature of the SyncAggregatorSelectionData derived from the contribution by the validator with index contribution_and_proof.aggregator_index
    let selection_data = SyncAggregatorSelectionData {
        slot: contribution.message.contribution.slot,
        subcommittee_index: contribution.message.contribution.subcommittee_index,
    };

    contribution
        .message
        .selection_proof
        .verify(aggregator_pubkey, selection_data.tree_hash_root().as_ref())?;

    

    Ok(ValidationResult::Accept)
}
