use anyhow::{Ok, anyhow};
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::{
    constants::MAX_BLOBS_PER_BLOCK_ELECTRA, electra::beacon_block::SignedBeaconBlock,
    misc::compute_start_slot_at_epoch,
};
use ream_storage::tables::{Field, Table};

#[derive(Debug)]
pub enum ValidationResult {
    Accept,
    Ignore,
    Reject,
}

pub async fn validate_beacon_block(
    beacon_chain: &BeaconChain,
    block: &SignedBeaconBlock,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    let latest_block_in_db = store.db.get_latest_block()?;
    let latest_state_in_db = store.db.get_latest_state()?;

    if block.message.slot < latest_block_in_db.message.slot {
        return Ok(ValidationResult::Ignore);
    }

    let start_slot_at_epoch =
        compute_start_slot_at_epoch(store.db.finalized_checkpoint_provider().get()?.epoch);

    if block.message.slot < start_slot_at_epoch {
        return Ok(ValidationResult::Ignore);
    }

    let validator = latest_state_in_db
        .validators
        .get(block.message.proposer_index as usize)
        .ok_or(anyhow!("Invalid proposer index"))?;

    if beacon_chain
        .cached_proposer_signature
        .read()
        .await
        .contains_key(&(validator.public_key.clone(), block.message.slot))
    {
        return Ok(ValidationResult::Ignore);
    }

    if !latest_state_in_db.verify_block_signature(block)? {
        return Ok(ValidationResult::Reject);
    }

    if let Some(parent) = store
        .db
        .beacon_block_provider()
        .get(block.message.parent_root)?
    {
        if block.message.slot > parent.message.slot + 1 {
            return Ok(ValidationResult::Reject);
        }
    } else {
        return Ok(ValidationResult::Ignore);
    }

    let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
    if !store.get_checkpoint_block(block.message.parent_root, finalized_checkpoint.epoch)?
        == finalized_checkpoint.root
    {
        return Ok(ValidationResult::Reject);
    }

    if !latest_state_in_db.get_beacon_proposer_index(Some(block.message.slot))?
        == block.message.proposer_index
    {
        return Ok(ValidationResult::Reject);
    }

    if !block.message.body.execution_payload.timestamp
        == latest_state_in_db.compute_timestamp_at_slot(block.message.slot)
    {
        return Ok(ValidationResult::Reject);
    }

    let proposer_bls_execution_change = &block
        .message
        .body
        .bls_to_execution_changes
        .get(block.message.proposer_index as usize)
        .ok_or(anyhow!("Invalid index for signed bls to execution change"))?
        .message;

    if beacon_chain
        .cached_bls_to_execution_signature
        .read()
        .await
        .contains_key(&(validator.public_key.clone(), block.message.proposer_index))
    {
        return Ok(ValidationResult::Ignore);
    }

    if block.message.body.blob_kzg_commitments.len() > MAX_BLOBS_PER_BLOCK_ELECTRA as usize {
        return Ok(ValidationResult::Reject);
    }

    beacon_chain
        .cached_proposer_signature
        .blocking_write()
        .insert(
            (validator.public_key.clone(), block.message.slot),
            block.signature.clone(),
        );

    beacon_chain
        .cached_bls_to_execution_signature
        .blocking_write()
        .insert(
            (validator.public_key.clone(), block.message.proposer_index),
            proposer_bls_execution_change.clone(),
        );

    Ok(ValidationResult::Accept)
}
