use anyhow::anyhow;
use ream_bls::traits::Verifiable;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    data_column_sidecar::DataColumnSidecar, electra::beacon_state::BeaconState,
};
use ream_consensus_misc::{
    constants::beacon::{DOMAIN_BEACON_PROPOSER, GENESIS_SLOT},
    misc::{compute_epoch_at_slot, compute_signing_root, compute_start_slot_at_epoch},
};
use ream_network_spec::networks::beacon_network_spec;
use ream_polynomial_commitments::handlers::verify_data_column_sidecar_kzg_proofs;
use ream_storage::{
    cache::BeaconCacheDB,
    tables::{field::REDBField, table::REDBTable},
};

use super::result::ValidationResult;

pub async fn validate_data_column_sidecar_full(
    data_column_sidecar: &DataColumnSidecar,
    beacon_chain: &BeaconChain,
    current_time_ms: u64,
    subnet_id: u64,
    cached_db: &BeaconCacheDB,
) -> anyhow::Result<ValidationResult> {
    let header = &data_column_sidecar.signed_block_header.message;
    let tuple = (
        header.slot,
        header.proposer_index,
        data_column_sidecar.index,
    );
    if cached_db
        .seen_data_column_sidecars
        .read()
        .await
        .contains(&tuple)
    {
        return Ok(ValidationResult::Ignore(
            "Already seen sidecar from this proposer for this slot and index".to_string(),
        ));
    }

    if !data_column_sidecar.verify() {
        return Ok(ValidationResult::Reject(
            "Data column sidecar failed basic verification".to_string(),
        ));
    }

    if subnet_id != data_column_sidecar.compute_subnet() {
        return Ok(ValidationResult::Reject(
            "Column sidecar not for correct subnet".to_string(),
        ));
    }

    let store = beacon_chain.store.lock().await;
    let head_root = store.get_head()?;
    let state: BeaconState = store
        .db
        .state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    if !is_not_from_future_slot(&state, header.slot, current_time_ms) {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a future slot".to_string(),
        ));
    }

    let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
    if header.slot <= compute_start_slot_at_epoch(finalized_checkpoint.epoch) {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a slot less than or equal to the latest finalized slot"
                .to_string(),
        ));
    }

    let Some(proposer) = usize::try_from(header.proposer_index)
        .ok()
        .and_then(|proposer_index| state.validators.get(proposer_index))
    else {
        return Ok(ValidationResult::Reject(
            "Sidecar proposer index out of range".to_string(),
        ));
    };

    let domain = state.get_domain(
        DOMAIN_BEACON_PROPOSER,
        Some(compute_epoch_at_slot(header.slot)),
    );
    let signing_root = compute_signing_root(header.clone(), domain);
    if !matches!(
        data_column_sidecar
            .signed_block_header
            .signature
            .verify(&proposer.public_key, signing_root.as_ref()),
        Ok(true)
    ) {
        return Ok(ValidationResult::Reject(
            "Invalid proposer signature on data column sidecar's block header".to_string(),
        ));
    }

    let Some(parent_block) = store.db.block_provider().get(header.parent_root)? else {
        return Ok(ValidationResult::Ignore(
            "Parent block not seen".to_string(),
        ));
    };

    let Some(mut parent_state) = store.db.state_provider().get(header.parent_root)? else {
        return Ok(ValidationResult::Reject(
            "Sidecar's parent failed validation".to_string(),
        ));
    };

    if header.slot <= parent_block.message.slot {
        return Ok(ValidationResult::Reject(
            "Sidecar slot not higher than parent block's slot".to_string(),
        ));
    }

    if store.get_checkpoint_block(header.parent_root, finalized_checkpoint.epoch)?
        != finalized_checkpoint.root
    {
        return Ok(ValidationResult::Reject(
            "Finalized checkpoint is not an ancestor of the sidecar's block".to_string(),
        ));
    }

    if !data_column_sidecar.verify_inclusion_proof() {
        return Ok(ValidationResult::Reject(
            "Invalid data column sidecar inclusion proof".to_string(),
        ));
    }

    if !matches!(
        verify_data_column_sidecar_kzg_proofs(data_column_sidecar),
        Ok(true)
    ) {
        return Ok(ValidationResult::Reject(
            "Invalid KZG proofs for data column sidecar".to_string(),
        ));
    }

    if let Err(err) = parent_state.process_slots(header.slot) {
        return Ok(ValidationResult::Ignore(format!(
            "Could not advance parent state to sidecar slot: {err:?}"
        )));
    }

    match parent_state.get_beacon_proposer_index(None) {
        Ok(expected_index) => {
            if expected_index != header.proposer_index {
                return Ok(ValidationResult::Reject(format!(
                    "Wrong proposer index: slot {}: expected {expected_index}, got {}",
                    header.slot, header.proposer_index
                )));
            }
        }
        Err(err) => {
            return Ok(ValidationResult::Ignore(format!(
                "Could not get proposer index: {err:?}"
            )));
        }
    }

    // Re-check under the write lock in case another validation inserted the tuple
    // after the initial duplicate check.
    let mut seen = cached_db.seen_data_column_sidecars.write().await;
    if seen.contains(&tuple) {
        return Ok(ValidationResult::Ignore(
            "Duplicate data column sidecar for (slot, proposer_index, index)".to_string(),
        ));
    }
    seen.put(tuple, ());

    Ok(ValidationResult::Accept)
}

fn is_not_from_future_slot(state: &BeaconState, slot: u64, current_time_ms: u64) -> bool {
    let network_spec = beacon_network_spec();
    let slots_since_genesis = slot.saturating_sub(GENESIS_SLOT);
    let slot_time_ms = state
        .genesis_time
        .saturating_mul(1000)
        .saturating_add(slots_since_genesis.saturating_mul(network_spec.slot_duration_ms));

    current_time_ms.saturating_add(network_spec.maximum_gossip_clock_disparity) >= slot_time_ms
}
