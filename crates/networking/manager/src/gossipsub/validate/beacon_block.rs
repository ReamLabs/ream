use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    execution_engine::new_payload_request::NewPayloadRequest,
};
use ream_consensus_misc::{
    constants::MAX_BLOBS_PER_BLOCK_ELECTRA, misc::compute_start_slot_at_epoch,
};
use ream_execution_engine::rpc_types::payload_status::PayloadStatus;
use ream_storage::{
    cache::{AddressSlotIdentifier, CachedDB},
    tables::{Field, Table},
};

use super::result::ValidationResult;

pub async fn validate_gossip_beacon_block(
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
    block: &SignedBeaconBlock,
) -> anyhow::Result<ValidationResult> {
    let latest_state = beacon_chain.store.lock().await.db.get_latest_state()?;

    // Validate incoming block
    match validate_beacon_block(beacon_chain, cached_db, block, &latest_state, false).await? {
        ValidationResult::Accept => {}
        ValidationResult::Ignore(reason) => {
            return Ok(ValidationResult::Ignore(reason));
        }
        ValidationResult::Reject(reason) => {
            return Ok(ValidationResult::Reject(reason));
        }
    }

    let (parent_block, parent_state) = {
        let store = beacon_chain.store.lock().await;
        let Some(parent_block) = store
            .db
            .beacon_block_provider()
            .get(block.message.parent_root)?
        else {
            return Err(anyhow!("failed to get parent block"));
        };

        let Some(parent_state) = store
            .db
            .beacon_state_provider()
            .get(block.message.parent_root)?
        else {
            return Err(anyhow!("failed to get parent state"));
        };

        (parent_block, parent_state)
    };

    // Validate parent block [block.message.parent_root]
    match validate_beacon_block(beacon_chain, cached_db, &parent_block, &parent_state, true).await?
    {
        ValidationResult::Accept => {}
        ValidationResult::Ignore(reason) => {
            return Ok(ValidationResult::Ignore(reason));
        }
        ValidationResult::Reject(reason) => {
            return Ok(ValidationResult::Reject(reason));
        }
    };

    let Some(validator) = latest_state
        .validators
        .get(block.message.proposer_index as usize)
    else {
        return Ok(ValidationResult::Reject("Validator not found".to_string()));
    };

    cached_db.seen_proposer_signature.write().await.put(
        AddressSlotIdentifier {
            address: validator.public_key.clone(),
            slot: block.message.slot,
        },
        block.signature.clone(),
    );

    for signed_bls_execution_change in block.message.body.bls_to_execution_changes.iter() {
        let validator =
            &latest_state.validators[signed_bls_execution_change.message.validator_index as usize];

        cached_db.seen_bls_to_execution_signature.write().await.put(
            AddressSlotIdentifier {
                address: validator.public_key.clone(),
                slot: block.message.slot,
            },
            signed_bls_execution_change.message.clone(),
        );
    }

    Ok(ValidationResult::Accept)
}

pub async fn validate_beacon_block(
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
    block: &SignedBeaconBlock,
    state: &BeaconState,
    is_parent: bool,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    // [IGNORE] The block is not from a future slot.
    if block.message.slot > store.get_current_slot()? {
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

    let Some(validator) = state.validators.get(block.message.proposer_index as usize) else {
        return Ok(ValidationResult::Reject("Validator not found".to_string()));
    };

    // [IGNORE] The block is the first block with valid signature received for the proposer for the
    // slot.
    if cached_db
        .seen_proposer_signature
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
    match state.verify_block_header_signature(&block.signed_header()) {
        Ok(true) => {}
        Ok(false) => {
            return Ok(ValidationResult::Reject("Invalid signature".to_string()));
        }
        Err(err) => {
            return Ok(ValidationResult::Reject(format!(
                "Signature verification failed: {err}"
            )));
        }
    }

    match store
        .db
        .beacon_block_provider()
        .get(block.message.parent_root)?
    {
        Some(parent_block) => {
            // [REJECT] The block is from a higher slot than its parent.
            if block.message.slot <= parent_block.message.slot {
                return Ok(ValidationResult::Reject(
                    "Block is not from a higher slot".to_string(),
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

    #[cfg(not(feature = "disable_ancestor_validation"))]
    {
        let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
        // [REJECT] The current finalized_checkpoint is an ancestor of block.
        if store.get_checkpoint_block(block.message.parent_root, finalized_checkpoint.epoch)?
            != finalized_checkpoint.root
        {
            return Ok(ValidationResult::Reject(
                "Finalized checkpoint is not an ancestor".to_string(),
            ));
        }
    }

    // [REJECT] The block is proposed by the expected proposer_index for the block's slot.
    if state.get_beacon_proposer_index(Some(block.message.slot))? != block.message.proposer_index {
        return Ok(ValidationResult::Reject(
            "Proposer index is incorrect".to_string(),
        ));
    }

    // [REJECT] The block's execution payload timestamp is correct with respect to the slot.
    if block.message.body.execution_payload.timestamp
        != state.compute_timestamp_at_slot(block.message.slot)
    {
        return Ok(ValidationResult::Reject(
            "Execution payload timestamp is incorrect".to_string(),
        ));
    }

    // [IGNORE] The signed_bls_to_execution_change is the first valid signed bls to execution change
    // received for the validator with index.
    if cached_db
        .seen_bls_to_execution_signature
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

    // [REJECT] All of the conditions within process_bls_to_execution_change pass validation.
    for signed_proposer_bls_execution_change in block.message.body.bls_to_execution_changes.iter() {
        if state
            .validate_bls_to_execution_change(signed_proposer_bls_execution_change)
            .is_err()
        {
            return Ok(ValidationResult::Reject(
                "BLS to execution change is invalid".to_string(),
            ));
        }
    }

    // [REJECT] The length of KZG commitments is less than or equal to the limitation.
    if block.message.body.blob_kzg_commitments.len() > MAX_BLOBS_PER_BLOCK_ELECTRA as usize {
        return Ok(ValidationResult::Reject(
            "Length of KZG commitments is greater than the limit".to_string(),
        ));
    }

    if is_parent {
        if let Some(execution_enigne) = &beacon_chain.execution_engine {
            let mut versioned_hashes = vec![];
            for commitment in block.message.body.blob_kzg_commitments.iter() {
                versioned_hashes.push(commitment.calculate_versioned_hash());
            }

            let payload_verification_status = execution_enigne
                .notify_new_payload(NewPayloadRequest {
                    execution_payload: block.message.body.execution_payload.clone(),
                    versioned_hashes,
                    parent_beacon_block_root: block.message.parent_root,
                    execution_requests: block.message.body.execution_requests.clone(),
                })
                .await?;

            match payload_verification_status {
                // If execution_payload verification of block's parent by an execution node is not
                // complete: [REJECT] The block's parent passes all validation (excluding
                // execution node verification of the block.body.execution_payload)
                PayloadStatus::Valid | PayloadStatus::Accepted | PayloadStatus::Syncing => {
                    return Ok(ValidationResult::Accept);
                }
                // Otherwise:
                // [IGNORE] The block's parent passes all validation (including execution node
                // verification of the block.body.execution_payload).
                PayloadStatus::InvalidBlockHash | PayloadStatus::Invalid => {
                    return Ok(ValidationResult::Reject(
                        "Execution payload is invalid".to_string(),
                    ));
                }
            }
        }
    }

    drop(store);

    Ok(ValidationResult::Accept)
}
