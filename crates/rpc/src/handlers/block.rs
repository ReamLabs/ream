use alloy_primitives::B256;
use ream_consensus::{constants::SLOTS_PER_EPOCH, deneb::beacon_block::BeaconBlock};
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use crate::types::{
    errors::ApiError,
    id::ID,
    response::{
        BeaconResponse, BeaconVersionedResponse, BlockRewardsData, BlockRewardsResponse,
        RootResponse,
    },
};

pub async fn get_block_root_from_id(block_id: ID, db: &ReamDB) -> Result<B256, ApiError> {
    let block_root = match block_id {
        ID::Finalized => {
            let finalized_checkpoint = db
                .finalized_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Finalized checkpoint not found"))
                })?;

            Ok(Some(finalized_checkpoint.root))
        }
        ID::Justified => {
            let justified_checkpoint = db
                .justified_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Justified checkpoint not found"))
                })?;

            Ok(Some(justified_checkpoint.root))
        }
        ID::Head | ID::Genesis => {
            return Err(ApiError::NotFound(format!(
                "This ID type is currently not supported: {block_id:?}"
            )));
        }
        ID::Slot(slot) => db.slot_index_provider().get(slot),
        ID::Root(root) => Ok(Some(root)),
    }
    .map_err(|_| ApiError::InternalError)?
    .ok_or(ApiError::NotFound(format!(
        "Failed to find `block_root` from {block_id:?}"
    )))?;

    Ok(block_root)
}

async fn get_beacon_block_from_id(block_id: ID, db: &ReamDB) -> Result<BeaconBlock, ApiError> {
    let block_root = get_block_root_from_id(block_id, db).await?;

    db.beacon_block_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon block` from {block_root:?}"
        )))
}

/// Called by `/eth/v2/beacon/{block_id}/attestations` to get block attestations
pub async fn get_block_attestations(block_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let beacon_block = get_beacon_block_from_id(block_id, &db).await?;

    Ok(with_status(
        BeaconVersionedResponse::json(beacon_block.body.attestations),
        StatusCode::OK,
    ))
}

/// Called by `/blocks/<block_id>/root` to get the Tree hash of the Block.
pub async fn get_block_root(block_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let block_root = get_block_root_from_id(block_id, &db).await?;
    Ok(with_status(
        BeaconResponse::json(RootResponse { root: block_root }),
        StatusCode::OK,
    ))
}

// Called by `/beacon/blocks/{block_id}/rewards` to get the block rewards response
pub async fn get_block_rewards(block_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let block_root = get_block_root_from_id(block_id, &db).await?;
    let beacon_block = db
        .beacon_block_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon block` from {block_root:?}"
        )))?;

    // Get finalized checkpoint to determine if block is finalized
    let finalized_checkpoint = db
        .finalized_checkpoint_provider()
        .get()
        .map_err(|_| ApiError::InternalError)?
        .ok_or_else(|| ApiError::NotFound(String::from("Finalized checkpoint not found")))?;

    // Check if block is finalized using epoch as the slot number
    let finalized_epoch_start_slot = finalized_checkpoint.epoch * SLOTS_PER_EPOCH;
    let is_finalized = beacon_block.slot <= finalized_epoch_start_slot;

    let response = BlockRewardsResponse {
        execution_optimistic: false,
        finalized: is_finalized,
        data: BlockRewardsData {
            proposer_index: beacon_block.proposer_index.to_string(),
            total: "0".to_string(), // Placeholder; To implement the calculate block reward logic
            attestations: beacon_block.body.attestations.len().to_string(),
            sync_aggregate: beacon_block
                .body
                .sync_aggregate
                .sync_committee_bits
                .num_set_bits()
                .to_string(),
            proposer_slashings: beacon_block.body.proposer_slashings.len().to_string(),
            attester_slashings: beacon_block.body.attester_slashings.len().to_string(),
        },
    };

    Ok(with_status(BeaconResponse::json(response), StatusCode::OK))
}
