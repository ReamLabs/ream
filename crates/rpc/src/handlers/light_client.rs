use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path, Query},
};
use alloy_primitives::B256;
use ream_beacon_api_types::{error::ApiError, responses::DataVersionedResponse};
use ream_consensus::constants::{EPOCHS_PER_SYNC_COMMITTEE_PERIOD, SLOTS_PER_EPOCH};
use ream_light_client::{bootstrap::LightClientBootstrap, update::LightClientUpdate};
use ream_storage::{db::ReamDB, tables::Table};
use tree_hash::TreeHash;

pub const MAX_REQUEST_LIGHT_CLIENT_UPDATES: u64 = 128;

#[get("/beacon/light_client/bootstrap/{block_root}")]
pub async fn get_light_client_bootstrap(
    db: Data<ReamDB>,
    block_root: Path<B256>,
) -> Result<impl Responder, ApiError> {
    let block_root = block_root.into_inner();
    let beacon_block = db
        .beacon_block_provider()
        .get(block_root)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get block by block_root, error: {err:?}"))
        })?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Failed to find `beacon block` from {block_root:?}"))
        })?;

    let beacon_state = db
        .beacon_state_provider()
        .get(block_root)
        .map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to get beacon_state from block_root, error: {err:?}"
            ))
        })?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon_state` from {block_root:?}"
        )))?;

    let light_client_bootstrap =
        LightClientBootstrap::new(&beacon_state, &beacon_block).map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to create light client bootstrap, error: {err:?}"
            ))
        })?;

    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(light_client_bootstrap)))
}

#[get("/beacon/light_client/updates")]
pub async fn get_light_client_updates(
    db: Data<ReamDB>,
    start_period: Query<u64>,
    count: Query<u64>,
) -> Result<impl Responder, ApiError> {
    let start_period: u64 = start_period.into_inner();
    let count = std::cmp::min(count.into_inner(), MAX_REQUEST_LIGHT_CLIENT_UPDATES);

    let mut updates = Vec::new();

    for period in start_period..start_period + count {
        let slot = period * EPOCHS_PER_SYNC_COMMITTEE_PERIOD * SLOTS_PER_EPOCH;
        let block_root = db
            .slot_index_provider()
            .get(slot)
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get block root for slot, error: {err:?}"
                ))
            })?
            .ok_or_else(|| {
                ApiError::NotFound(format!(
                    "No block root found for slot {slot} (period {start_period})",
                ))
            })?;

        let block = db
            .beacon_block_provider()
            .get(block_root)
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get beacon_block from block_root, error: {err:?}"
                ))
            })?
            .ok_or(ApiError::NotFound(format!(
                "Failed to find beacon_block from {block_root:?}"
            )))?;

        let state = db
            .beacon_state_provider()
            .get(block_root)
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get beacon_state from block_root, error: {err:?}"
                ))
            })?
            .ok_or(ApiError::NotFound(format!(
                "Failed to find beacon_state from {block_root:?}"
            )))?;

        let attested_block = db
            .beacon_block_provider()
            .get(block.message.parent_root)
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get attested_block from block.message.parent_root, error: {err:?}"
                ))
            })?
            .ok_or(ApiError::NotFound(format!(
                "Failed to find attested_block from {:?}",
                block.message.parent_root
            )))?;

        let attested_block_root = attested_block.tree_hash_root();
        let attested_state = db
            .beacon_state_provider()
            .get(attested_block_root)
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get attested_state from attested_block_root, error: {err:?}"
                ))
            })?
            .ok_or(ApiError::NotFound(format!(
                "Failed to find attested_state from {attested_block_root:?}"
            )))?;

        let finalized_block = db
            .beacon_block_provider()
            .get(attested_state.finalized_checkpoint.root)
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get finalized_block from attested_state.finalized_checkpoint.root, error: {err:?}"
                ))
            })?
            .ok_or(ApiError::NotFound(format!(
                "Failed to find finalized_block from {:?}",attested_state.finalized_checkpoint.root
            )))?;

        updates.push(
            LightClientUpdate::new(
                state,
                block,
                attested_state,
                attested_block,
                Some(finalized_block),
            )
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to create light client bootstrap, error: {err:?}"
                ))
            })?,
        );
    }
    if updates.len() > (count as usize) {
        return Err(ApiError::NotFound(
            "No light client updates found in requested range".into(),
        ));
    }
    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(updates)))
}
