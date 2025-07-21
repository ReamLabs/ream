use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus_beacon::{blob_sidecar::BlobSidecar, electra::beacon_state::BeaconState};
use ream_consensus_misc::{
    constants::MAX_BLOBS_PER_BLOCK_ELECTRA, misc::compute_start_slot_at_epoch,
};
use ream_polynomial_commitments::handlers::verify_blob_kzg_proof_batch;
use ream_storage::{
    cache::CachedDB,
    tables::{Field, Table},
};
use ream_validator::blob_sidecars::compute_subnet_for_blob_sidecar;

use super::result::ValidationResult;

pub async fn validate_blob_sidecar(
    beacon_chain: &BeaconChain,
    blob_sidecar: &BlobSidecar,
    subnet_id: u64,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    // [REJECT] The sidecar's index is consistent with MAX_BLOBS_PER_BLOCK
    if blob_sidecar.index >= MAX_BLOBS_PER_BLOCK_ELECTRA {
        return Ok(ValidationResult::Reject(
            "Blob index exceeds MAX_BLOBS_PER_BLOCK".to_string(),
        ));
    }

    // [REJECT] The sidecar is for the correct subnet
    if compute_subnet_for_blob_sidecar(blob_sidecar.index) != subnet_id {
        return Ok(ValidationResult::Reject(
            "Blob sidecar not for correct subnet".to_string(),
        ));
    }

    let header = &blob_sidecar.signed_block_header.message;
    let store = beacon_chain.store.lock().await;

    // [IGNORE] The sidecar is not from a future slot
    if header.slot > store.get_current_slot()? {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a future slot".to_string(),
        ));
    }

    let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;

    // [IGNORE] The sidecar is from a slot greater than the latest finalized slot
    if header.slot <= compute_start_slot_at_epoch(finalized_checkpoint.epoch) {
        return Ok(ValidationResult::Ignore(
            "The sidecar is from a slot less than the latest finalized slot".to_string(),
        ));
    }

    let head_root = store.get_head()?;
    let state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    // [REJECT] The proposer signature of blob_sidecar.signed_block_header, is valid with respect to
    // the block_header.proposer_index pubkey.
    if !state.verify_block_header_signature(&blob_sidecar.signed_block_header)? {
        return Ok(ValidationResult::Reject(
            "Invalid proposer signature on blob sidecar's block header".to_string(),
        ));
    }

    // [IGNORE] The sidecar's block's parent (defined by block_header.parent_root) has been seen
    let Some(parent_block) = store.db.beacon_block_provider().get(header.parent_root)? else {
        return Ok(ValidationResult::Ignore(
            "Parent block not seen".to_string(),
        ));
    };

    // [REJECT] The sidecar's block's parent passes validation
    // If we store the parent block then it has passed validation

    // [REJECT] The sidecar is from a higher slot than the sidecar's block's parent
    if header.slot <= parent_block.message.slot {
        return Ok(ValidationResult::Reject(
            "Sidecar slot not higher than parent block's slot".to_string(),
        ));
    }

    // [REJECT] The current finalized_checkpoint is an ancestor of the sidecar's block
    if store.get_checkpoint_block(header.parent_root, finalized_checkpoint.epoch)?
        != finalized_checkpoint.root
    {
        return Ok(ValidationResult::Reject(
            "Finalized checkpoint is not an ancestor of the sidecar's block".to_string(),
        ));
    }

    // [REJECT] The sidecar's inclusion proof is valid as verified by
    if !blob_sidecar.verify_blob_sidecar_inclusion_proof() {
        return Ok(ValidationResult::Reject(
            "Invalid blob sidecar inclusion proof".to_string(),
        ));
    }

    // [REJECT] The sidecar's blob is valid as verified by
    if !verify_blob_kzg_proof_batch(
        &[blob_sidecar.blob.clone()],
        &[blob_sidecar.kzg_commitment],
        &[blob_sidecar.kzg_proof],
    )? {
        return Ok(ValidationResult::Reject(
            "Invalid blob for blob sidecar".to_string(),
        ));
    }

    // [IGNORE] The sidecar is the first sidecar for the tuple (block_header.slot,
    // block_header.proposer_index, blob_sidecar.index) with valid header signature, sidecar
    // inclusion proof, and kzg proof.
    let tuple = (header.slot, header.proposer_index, blob_sidecar.index);
    let mut seen = cached_db.cached_seen_blob_sidecars.write().await;
    if seen.contains(&tuple) {
        return Ok(ValidationResult::Ignore(
            "Duplicate blob sidecar for (slot, proposer_index, index)".to_string(),
        ));
    }
    seen.put(tuple, ());

    // [REJECT or IGNORE] The sidecar is proposed by the expected proposer_index for the block's
    // slot in the context of the current shuffling
    match state.get_beacon_proposer_index(Some(header.slot)) {
        Ok(expected_index) => {
            if expected_index != header.proposer_index {
                return Ok(ValidationResult::Reject(format!(
                    "Wrong proposer index: slot {}: expected {}, got {}",
                    header.slot, expected_index, header.proposer_index
                )));
            }
        }
        Err(err) => {
            return Ok(ValidationResult::Reject(format!(
                "Could not verify proposer index: {err:?}"
            )));
        }
    }

    Ok(ValidationResult::Accept)
}
