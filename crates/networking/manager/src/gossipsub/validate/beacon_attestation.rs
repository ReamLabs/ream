use anyhow::anyhow;
use ream_bls::traits::Verifiable;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    electra::beacon_state::BeaconState, single_attestation::SingleAttestation,
};
use ream_consensus_misc::{
    constants::DOMAIN_BEACON_ATTESTER,
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use ream_storage::{
    cache::{AtestationKey, CachedDB},
    tables::{Field, Table},
};
use ream_validator_beacon::attestation::compute_subnet_for_attestation;

use super::result::ValidationResult;

pub async fn validate_beacon_attestation(
    attestation: &SingleAttestation,
    beacon_chain: &BeaconChain,
    attestation_subnet_id: u64,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    let head_root = store.get_head()?;
    let state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    let index = attestation.committee_index;
    let committees_per_slot = state.get_committee_count_per_slot(attestation.data.target.epoch);

    // [REJECT] The committee index is within the expected range
    if index >= committees_per_slot {
        return Ok(ValidationResult::Reject(
            "The committee index is not within the expected range".to_string(),
        ));
    }

    // [REJECT] The attestation is for the correct subnet
    if compute_subnet_for_attestation(committees_per_slot, attestation.data.slot, index)
        != attestation_subnet_id
    {
        return Ok(ValidationResult::Reject(
            "The attestation is not for the correct subnet".to_string(),
        ));
    }

    let block = store
        .db
        .beacon_block_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("Could not get block for head root: {head_root}"))?;

    let current_slot = block.message.slot;

    // [IGNORE] attestation.data.slot is equal to or earlier than the current_slot (with a
    // MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
    if attestation.data.slot > current_slot {
        return Ok(ValidationResult::Ignore(
            "Attestation is from a future slot".to_string(),
        ));
    }

    // [IGNORE] the epoch of attestation.data.slot is either the current or previous epoch (with a
    // MAXIMUM_GOSSIP_CLOCK_DISPARITY allowance)
    let attestation_epoch = compute_epoch_at_slot(attestation.data.slot);
    let current_epoch = state.get_current_epoch();
    let previous_epoch = state.get_previous_epoch();

    if attestation_epoch != current_epoch && attestation_epoch != previous_epoch {
        return Ok(ValidationResult::Ignore(
            "Attestation is from a epoch too far in the past".to_string(),
        ));
    }

    // [REJECT] The attestation's epoch matches its target
    if attestation.data.target.epoch != attestation_epoch {
        return Ok(ValidationResult::Reject(
            "The attestation's epoch doesn't match its target".to_string(),
        ));
    }

    // [REJECT] attestation.data.index == 0
    if index == 0 {
        return Ok(ValidationResult::Reject(
            "Committee index must not be 0".to_string(),
        ));
    }

    // [REJECT] The attester is a member of the committee
    if !state
        .get_beacon_committee(attestation.data.slot, index)?
        .contains(&(attestation.attester_index))
    {
        return Ok(ValidationResult::Reject(
            "The attester is not a member of the committee".to_string(),
        ));
    }

    // [IGNORE] There has been no other valid attestation seen on an attestation subnet that has an
    // identical attestation.data.target.epoch and participating validator index.
    let attestation_key = AtestationKey {
        attestation_subnet_id,
        target_epoch: attestation.data.target.epoch,
        participating_validator_index: attestation.attester_index,
    };
    if cached_db
        .seen_attestations
        .read()
        .await
        .contains(&attestation_key)
    {
        return Ok(ValidationResult::Ignore(
            "There has been no other valid attestation seen".to_string(),
        ));
    }

    // [REJECT] The signature of attestation is valid.
    let validator = state
        .validators
        .get(attestation.attester_index as usize)
        .ok_or_else(|| anyhow!("Could not get validator"))?;

    let domain = state.get_domain(DOMAIN_BEACON_ATTESTER, Some(attestation.data.target.epoch));
    let signing_root = compute_signing_root(&attestation.data, domain);

    let signature_valid = attestation
        .signature
        .verify(&validator.public_key, signing_root.as_slice())?;

    if !signature_valid {
        return Ok(ValidationResult::Reject(
            "Invalid attestation signature".to_string(),
        ));
    }

    // [IGNORE] The block being voted for (aggregate.data.beacon_block_root) has been seen (via
    // gossip or non-gossip sources) (a client MAY queue aggregates for processing once block is
    // retrieved).
    if store
        .db
        .beacon_block_provider()
        .get(attestation.data.beacon_block_root)?
        .is_none()
    {
        return Ok(ValidationResult::Ignore(
            "The block being voted for has not been seen".to_string(),
        ));
    }

    // [REJECT] The block being voted for (aggregate.data.beacon_block_root) passes validation.
    // All blocks stored passed validation

    // [REJECT] The attestation's target block is an ancestor of the block named in the LMD vote
    if store.get_checkpoint_block(
        attestation.data.beacon_block_root,
        attestation.data.target.epoch,
    )? != attestation.data.target.root
    {
        return Ok(ValidationResult::Reject(
            "The target block is not an ancestor of the LMD vote block".to_string(),
        ));
    }

    // [IGNORE] The current finalized_checkpoint is an ancestor of the block defined by
    // aggregate.data.beacon_block_root
    let finalized_checpoint = store.db.finalized_checkpoint_provider().get()?;
    if store.get_checkpoint_block(
        attestation.data.beacon_block_root,
        finalized_checpoint.epoch,
    )? != finalized_checpoint.root
    {
        return Ok(ValidationResult::Ignore(
            "Finalized checkpoint is not an ancestor of the block defined by aggregate.data.beacon_block_root".to_string(),
        ));
    }

    cached_db
        .seen_attestations
        .write()
        .await
        .put(attestation_key, ());
    Ok(ValidationResult::Accept)
}
