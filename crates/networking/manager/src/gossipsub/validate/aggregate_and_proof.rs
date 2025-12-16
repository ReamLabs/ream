use anyhow::anyhow;
use ream_bls::{PublicKey, traits::Verifiable};
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::{
    constants::beacon::{
        ATTESTATION_PROPAGATION_SLOT_RANGE, DOMAIN_AGGREGATE_AND_PROOF, DOMAIN_BEACON_ATTESTER,
    },
    misc::{compute_epoch_at_slot, compute_signing_root, get_committee_indices},
};
use ream_network_spec::networks::beacon_network_spec;
use ream_storage::{
    cache::{AggregateAndProofKey, CachedDB},
    tables::table::REDBTable,
};
use ream_validator_beacon::{
    aggregate_and_proof::SignedAggregateAndProof, attestation::is_aggregator,
    constants::DOMAIN_SELECTION_PROOF,
};

use super::result::ValidationResult;

pub async fn validate_aggregate_and_proof(
    signed_aggregate_and_proof: &SignedAggregateAndProof,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    let head_root = store.get_head()?;
    let state: BeaconState = store
        .db
        .state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    let block = store
        .db
        .block_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("Could not get block for head root: {head_root}"))?;

    let current_slot = block.message.slot;
    let aggregate_and_proof = &signed_aggregate_and_proof.message;
    let attestation = &aggregate_and_proof.aggregate;
    let attestation_slot = attestation.data.slot;
    let attestation_epoch = compute_epoch_at_slot(attestation_slot);

    // Determine current fork
    let network_spec = beacon_network_spec();
    let current_epoch = state.get_current_epoch();
    let is_electra = current_epoch >= network_spec.electra_fork_epoch;
    let is_deneb = current_epoch >= network_spec.deneb_fork_epoch;

    // Get committee indices early for validation
    let committee_indices = get_committee_indices(&attestation.committee_bits);
    if committee_indices.is_empty() {
        return Ok(ValidationResult::Reject(
            "No committee index found in committee_bits".to_string(),
        ));
    }

    // [REJECT] For Electra: len(committee_indices) == 1
    // In Electra, aggregates must have exactly one committee bit set
    if is_electra && committee_indices.len() != 1 {
        return Ok(ValidationResult::Reject(
            "Electra: aggregate must have exactly one committee bit set".to_string(),
        ));
    }

    // [REJECT] For Electra: aggregate.data.index == 0
    // In Electra, the attestation data index must be 0
    if is_electra && attestation.data.index != 0 {
        return Ok(ValidationResult::Reject(
            "Electra: attestation data index must be 0".to_string(),
        ));
    }

    // For phase0/Altair/Bellatrix/Capella/Deneb pre-Electra, use attestation.data.index
    // For Electra, use the committee index from committee_bits
    let committee_index = if is_electra {
        committee_indices[0]
    } else {
        attestation.data.index
    };

    // [REJECT] The committee index is within the expected range
    let committee_count = state.get_committee_count_per_slot(attestation.data.target.epoch);
    if committee_index >= committee_count {
        return Ok(ValidationResult::Reject(
            "Committee index is not within the expected range".to_string(),
        ));
    }

    // Timing validation differs between pre-Deneb and post-Deneb
    if is_deneb {
        // Post-Deneb:
        // [IGNORE] aggregate.data.slot is equal to or earlier than the current_slot
        // (with a MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
        if attestation_slot > current_slot {
            return Ok(ValidationResult::Ignore(
                "Aggregate is from a future slot".to_string(),
            ));
        }

        // [IGNORE] the epoch of aggregate.data.slot is either the current or previous epoch
        // (with a MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
        let current_epoch = state.get_current_epoch();
        let previous_epoch = state.get_previous_epoch();
        if attestation_epoch != current_epoch && attestation_epoch != previous_epoch {
            return Ok(ValidationResult::Ignore(
                "Aggregate epoch is not current or previous epoch".to_string(),
            ));
        }
    } else {
        // Pre-Deneb:
        // [IGNORE] aggregate.data.slot is within the last ATTESTATION_PROPAGATION_SLOT_RANGE slots
        // (with a MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
        if attestation_slot + ATTESTATION_PROPAGATION_SLOT_RANGE < current_slot {
            return Ok(ValidationResult::Ignore("Aggregate is too old".to_string()));
        }

        if attestation_slot > current_slot {
            return Ok(ValidationResult::Ignore(
                "Aggregate is from a future slot".to_string(),
            ));
        }
    }

    // [REJECT] The aggregate attestation's epoch matches its target
    if attestation.data.target.epoch != attestation_epoch {
        return Ok(ValidationResult::Reject(
            "The aggregate's epoch doesn't match its target".to_string(),
        ));
    }

    // [REJECT] The number of aggregation bits matches the committee size
    let committee = state.get_beacon_committee(attestation_slot, committee_index)?;

    // For aggregates spanning multiple committees, compute total expected length
    let mut total_expected_len = 0;
    for &idx in &committee_indices {
        let comm = state.get_beacon_committee(attestation_slot, idx)?;
        total_expected_len += comm.len();
    }

    if attestation.aggregation_bits.len() != total_expected_len {
        let actual_len = attestation.aggregation_bits.len();
        return Ok(ValidationResult::Reject(format!(
            "Aggregation bits length ({actual_len}) doesn't match committee size ({total_expected_len})"
        )));
    }

    // [REJECT] The attestation has participants -- that is, get_attesting_indices(state, aggregate)
    // >= 1
    let num_participants = attestation.aggregation_bits.num_set_bits();
    if num_participants == 0 {
        return Ok(ValidationResult::Reject(
            "The aggregate attestation has no participants".to_string(),
        ));
    }

    // [REJECT] aggregate_and_proof.selection_proof selects the validator as an aggregator for the
    // slot i.e., is_aggregator(state, aggregate.data.slot, index,
    // aggregate_and_proof.selection_proof) returns True
    let is_valid_aggregator = is_aggregator(
        &state,
        attestation_slot,
        committee_index,
        aggregate_and_proof.selection_proof.clone(),
    )?;

    if !is_valid_aggregator {
        return Ok(ValidationResult::Reject(
            "The validator is not an aggregator for the slot".to_string(),
        ));
    }

    // [REJECT] The aggregator's validator index is within the committee
    // i.e., aggregate_and_proof.aggregator_index in get_beacon_committee(state,
    // aggregate.data.slot, index)
    if !committee.contains(&aggregate_and_proof.aggregator_index) {
        return Ok(ValidationResult::Reject(
            "The aggregator is not in the committee".to_string(),
        ));
    }

    // [REJECT] The aggregate_and_proof.selection_proof is a valid signature of the
    // aggregate.data.slot by the validator with index aggregate_and_proof.aggregator_index.
    let aggregator_index = aggregate_and_proof.aggregator_index as usize;
    let validator = state
        .validators
        .get(aggregator_index)
        .ok_or_else(|| anyhow!("Aggregator validator not found"))?;

    let selection_proof_domain = state.get_domain(DOMAIN_SELECTION_PROOF, Some(attestation_epoch));
    let selection_proof_signing_root =
        compute_signing_root(attestation_slot, selection_proof_domain);

    let is_selection_proof_valid = aggregate_and_proof.selection_proof.verify(
        &validator.public_key,
        selection_proof_signing_root.as_slice(),
    )?;

    if !is_selection_proof_valid {
        return Ok(ValidationResult::Reject(
            "The selection proof is not a valid signature".to_string(),
        ));
    }

    // [REJECT] The aggregator signature, signed_aggregate_and_proof.signature, is valid.
    let aggregate_and_proof_domain =
        state.get_domain(DOMAIN_AGGREGATE_AND_PROOF, Some(attestation_epoch));
    let aggregate_and_proof_signing_root =
        compute_signing_root(aggregate_and_proof, aggregate_and_proof_domain);

    let is_aggregator_signature_valid = signed_aggregate_and_proof.signature.verify(
        &validator.public_key,
        aggregate_and_proof_signing_root.as_slice(),
    )?;

    if !is_aggregator_signature_valid {
        return Ok(ValidationResult::Reject(
            "The aggregator signature is not valid".to_string(),
        ));
    }

    // [IGNORE] There has been no other valid aggregate_and_proof seen with an identical
    // aggregate_and_proof.aggregator_index and aggregate.data.target.epoch.
    let aggregate_key = AggregateAndProofKey {
        aggregator_index: aggregate_and_proof.aggregator_index,
        target_epoch: attestation.data.target.epoch,
    };

    if cached_db
        .seen_aggregate_and_proof
        .read()
        .await
        .contains(&aggregate_key)
    {
        return Ok(ValidationResult::Ignore(
            "A valid aggregate_and_proof with identical aggregator_index and target_epoch has already been seen".to_string(),
        ));
    }

    // [REJECT] The signature of aggregate is valid.
    // The aggregate signature is valid for the message aggregate.data and the aggregate pubkey
    // derived from the participation info in aggregate.aggregation_bits.
    let mut committee_pubkeys: Vec<&PublicKey> = Vec::new();
    let mut committee_offset = 0;
    for &committee_idx in &committee_indices {
        let committee = state.get_beacon_committee(attestation_slot, committee_idx)?;
        for (i, validator_index) in committee.iter().enumerate() {
            if attestation
                .aggregation_bits
                .get(committee_offset + i)
                .unwrap_or(false)
            {
                committee_pubkeys.push(&state.validators[*validator_index as usize].public_key);
            }
        }
        committee_offset += committee.len();
    }

    // Should not be empty due to earlier check, but verify anyway
    if committee_pubkeys.is_empty() {
        return Ok(ValidationResult::Reject(
            "No aggregation bits set in the attestation".to_string(),
        ));
    }

    let aggregate_signature_domain =
        state.get_domain(DOMAIN_BEACON_ATTESTER, Some(attestation.data.target.epoch));
    let aggregate_signature_signing_root =
        compute_signing_root(&attestation.data, aggregate_signature_domain);

    let is_aggregate_signature_valid = attestation.signature.fast_aggregate_verify(
        committee_pubkeys,
        aggregate_signature_signing_root.as_slice(),
    )?;

    if !is_aggregate_signature_valid {
        return Ok(ValidationResult::Reject(
            "The aggregate signature is not valid".to_string(),
        ));
    }

    // [IGNORE] The block being voted for (aggregate.data.beacon_block_root) has been seen (via
    // gossip or non-gossip sources) (a client MAY queue aggregates for processing once block is
    // retrieved).
    if store
        .db
        .block_provider()
        .get(attestation.data.beacon_block_root)?
        .is_none()
    {
        return Ok(ValidationResult::Ignore(
            "The block being voted for has not been seen".to_string(),
        ));
    }

    // [REJECT] The block being voted for (aggregate.data.beacon_block_root) passes validation.
    // All blocks stored passed validation

    // [REJECT] The aggregate attestation's target block is an ancestor of the block named in the
    // LMD vote i.e., get_checkpoint_block(store, aggregate.data.beacon_block_root,
    // aggregate.data.target.epoch) == aggregate.data.target.root
    if store.get_checkpoint_block(
        attestation.data.beacon_block_root,
        attestation.data.target.epoch,
    )? != attestation.data.target.root
    {
        return Ok(ValidationResult::Reject(
            "The target block is not an ancestor of the LMD vote block".to_string(),
        ));
    }

    // Mark this aggregate_and_proof as seen
    cached_db
        .seen_aggregate_and_proof
        .write()
        .await
        .put(aggregate_key, ());

    Ok(ValidationResult::Accept)
}
