use anyhow::anyhow;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::data_column_sidecar::{DataColumnSidecar, NUMBER_OF_COLUMNS};
use ream_consensus_misc::{
    constants::beacon::MAX_BLOBS_PER_BLOCK, misc::compute_start_slot_at_epoch,
};
use ream_polynomial_commitments::handlers::verify_cell_kzg_proof_batch;
use ream_storage::{
    cache::CachedDB,
    tables::{field::REDBField, table::REDBTable},
};

use super::result::ValidationResult;

pub async fn validate_data_column_sidecar_full(
    data_column_sidecar: &DataColumnSidecar,
    beacon_chain: &BeaconChain,
    subnet_id: u64,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let result = validate_data_column_sidecar(data_column_sidecar).await?;
    if result != ValidationResult::Accept {
        return Ok(result);
    }

    if subnet_id != data_column_sidecar.compute_subnet() {
        return Ok(ValidationResult::Reject(
            "Column sidecar not for correct subnet".to_string(),
        ));
    }

    let header = &data_column_sidecar.signed_block_header.message;
    let store = beacon_chain.store.lock().await;

    if header.slot > store.get_current_slot()? {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a future slot".to_string(),
        ));
    }

    let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
    let head_root = store.get_head()?;
    let state = store
        .db
        .state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    if header.slot <= compute_start_slot_at_epoch(finalized_checkpoint.epoch) {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a slot less than or equal to the latest finalized slot"
                .to_string(),
        ));
    }

    if !state.verify_block_header_signature(&data_column_sidecar.signed_block_header)? {
        return Ok(ValidationResult::Reject(
            "Invalid proposer signature on data column sidecar's block header".to_string(),
        ));
    }

    // TODO [REJECT] The sidecar's block's parent (defined by block_header.parent_root) passes
    // validation.

    let Some(parent_block) = store.db.block_provider().get(header.parent_root)? else {
        return Ok(ValidationResult::Ignore(
            "Parent block not seen".to_string(),
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

    if !verify_cell_kzg_proof_batch(
        &data_column_sidecar.kzg_commitments,
        &vec![data_column_sidecar.index; data_column_sidecar.column.len()],
        &data_column_sidecar.column,
        &data_column_sidecar.kzg_proofs,
    )? {
        return Ok(ValidationResult::Reject(
            "Invalid KZG proofs for data column sidecar".to_string(),
        ));
    }

    let tuple = (
        header.slot,
        header.proposer_index,
        data_column_sidecar.index,
    );
    let mut seen = cached_db.seen_data_column_sidecars.write().await;
    if seen.contains(&tuple) {
        return Ok(ValidationResult::Ignore(
            "Duplicate data column sidecar for (slot, proposer_index, index)".to_string(),
        ));
    }
    seen.put(tuple, ());
    drop(seen);

    match state.get_beacon_proposer_index(Some(header.slot)) {
        Ok(expected_index) => {
            if expected_index != header.proposer_index {
                return Ok(ValidationResult::Reject(format!(
                    "Wrong proposer index: slot {}: expected {expected_index}, got {}",
                    header.slot, header.proposer_index
                )));
            }
        }
        Err(err) => {
            return Ok(ValidationResult::Reject(format!(
                "Could not get proposer index: {err:?}"
            )));
        }
    }

    Ok(ValidationResult::Accept)
}

/// Verify if the data column sidecar is valid.
async fn validate_data_column_sidecar(
    data_column_sidecar: &DataColumnSidecar,
) -> anyhow::Result<ValidationResult> {
    if data_column_sidecar.index >= NUMBER_OF_COLUMNS {
        return Ok(ValidationResult::Reject(
            "Column index exceeds NUMBER_OF_COLUMNS".to_string(),
        ));
    }

    if data_column_sidecar.kzg_commitments.is_empty() {
        return Ok(ValidationResult::Reject(
            "No KZG commitments in data column sidecar".to_string(),
        ));
    }

    // TODO dynamically get MAX_BLOBS_PER_BLOCK based on network spec for the epoch
    if data_column_sidecar.kzg_commitments.len() > MAX_BLOBS_PER_BLOCK {
        return Ok(ValidationResult::Reject(
            "Too many KZG commitments in data column sidecar".to_string(),
        ));
    }

    if data_column_sidecar.column.len() != data_column_sidecar.kzg_commitments.len()
        || data_column_sidecar.column.len() != data_column_sidecar.kzg_proofs.len()
    {
        return Ok(ValidationResult::Reject(
            "Mismatch in lengths of column, KZG commitments, and KZG proofs".to_string(),
        ));
    }

    Ok(ValidationResult::Accept)
}
