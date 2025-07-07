use alloy_primitives::B256;
use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::{constants::MAX_BLOBS_PER_BLOCK_ELECTRA, misc::compute_start_slot_at_epoch};
use ream_storage::{
    cache::{AddressSlotIdentifier, CachedDB},
    tables::{Field, Table},
};

use super::result::ValidationResult;

/// Validate the parent beacon block to avoid recursive validation.
pub async fn validate_parent_beacon_block(
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
    block_root: B256,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;
    let Some(block) = store.db.beacon_block_provider().get(block_root)? else {
        return Err(anyhow!("failed to get parent block"));
    };

    let current_global_slot_minus_one = store.get_current_slot()?;
    let Some(parent_state) = store.db.beacon_state_provider().get(block_root)? else {
        return Err(anyhow!("failed to get parent state"));
    };

    // [IGNORE] The block is not from a future slot.
    if block.message.slot > current_global_slot_minus_one {
        return Ok(ValidationResult::Ignore(
            "Block is from a future slot".to_string(),
        ));
    }

    // [IGNORE] The block is from a slot greater than the latest finalized slot.
    if block.message.slot
        <= compute_start_slot_at_epoch(store.db.finalized_checkpoint_provider().get()?.epoch)
    {
        return Ok(ValidationResult::Ignore(
            "Block is from a slot greater than the latest finalized slot".to_string(),
        ));
    }

    let validator = parent_state
        .validators
        .get(block.message.proposer_index as usize)
        .ok_or(anyhow!("Invalid proposer index"))?;

    // [IGNORE] The block is the first block with valid signature received for the proposer for the
    // slot.
    if cached_db
        .cached_proposer_signature
        .read()
        .await
        .contains(&AddressSlotIdentifier {
            address: validator.public_key.clone(),
            slot: block.message.slot,
        })
    {
        return Ok(ValidationResult::Ignore(
            "Signature already received".to_string(),
        ));
    }

    // [REJECT] The proposer signature, signed_beacon_block.signature, is valid with respect to the
    // proposer_index pubkey.
    if !parent_state.verify_block_signature(&block)? {
        return Ok(ValidationResult::Reject("Invalid signature".to_string()));
    }

    match store
        .db
        .beacon_block_provider()
        .get(block.message.parent_root)?
    {
        Some(parent_block) => {
            // [REJECT] The block is from a higher slot than its parent.
            if block.message.slot > parent_block.message.slot + 1 {
                return Ok(ValidationResult::Reject(
                    "Block is from a higher slot than expected".to_string(),
                ));
            }
        }
        None => {
            // [IGNORE] The block's parent (defined by block.parent_root) has been seen.
            return Ok(ValidationResult::Ignore(
                "Parent block not found".to_string(),
            ));
        }
    }

    let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
    // [REJECT] The current finalized_checkpoint is an ancestor of block.
    if store.get_checkpoint_block(block.message.parent_root, finalized_checkpoint.epoch)?
        != finalized_checkpoint.root
    {
        return Ok(ValidationResult::Reject(
            "Finalized checkpoint is not an ancestor".to_string(),
        ));
    }

    // [REJECT] The block is proposed by the expected proposer_index for the block's slot.
    if parent_state.get_beacon_proposer_index(Some(block.message.slot))?
        != block.message.proposer_index
    {
        return Ok(ValidationResult::Reject(
            "Proposer index is incorrect".to_string(),
        ));
    }

    // [REJECT] The block's execution payload timestamp is correct with respect to the slot.
    if block.message.body.execution_payload.timestamp
        != parent_state.compute_timestamp_at_slot(block.message.slot)
    {
        return Ok(ValidationResult::Reject(
            "Execution payload timestamp is incorrect".to_string(),
        ));
    }

    // [IGNORE] The signed_bls_to_execution_change is the first valid signed bls to execution change
    // received for the validator with index.
    if cached_db
        .cached_bls_to_execution_signature
        .read()
        .await
        .contains(&AddressSlotIdentifier {
            address: validator.public_key.clone(),
            slot: block.message.slot,
        })
    {
        return Ok(ValidationResult::Ignore(
            "Signature already received".to_string(),
        ));
    }

    // [REJECT] The length of KZG commitments is less than or equal to the limitation.
    if block.message.body.blob_kzg_commitments.len() > MAX_BLOBS_PER_BLOCK_ELECTRA as usize {
        return Ok(ValidationResult::Reject(
            "Length of KZG commitments is greater than the limit".to_string(),
        ));
    }

    Ok(ValidationResult::Accept)
}
