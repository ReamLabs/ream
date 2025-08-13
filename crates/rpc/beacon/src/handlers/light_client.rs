use actix_web::{
    HttpRequest, HttpResponse, Responder, get,
    web::{Data, Path, Query},
};
use alloy_primitives::B256;
use ream_api_types_beacon::{
    error::ApiError,
    responses::{
        DataVersionedResponse, ETH_CONSENSUS_VERSION_HEADER, JSON_CONTENT_TYPE, SSZ_CONTENT_TYPE,
        VERSION,
    },
};
use ream_consensus_misc::constants::beacon::{EPOCHS_PER_SYNC_COMMITTEE_PERIOD, SLOTS_PER_EPOCH};
use ream_light_client::{
    bootstrap::LightClientBootstrap, finality_update::LightClientFinalityUpdate,
    header::LightClientHeader, update::LightClientUpdate,
};
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use ssz::Encode;
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

        let attested_block_root = attested_block.message.tree_hash_root();
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

#[get("/beacon/light_client/finality_update")]
pub async fn get_light_client_finality_update(
    db: Data<ReamDB>,
    http_request: HttpRequest,
) -> Result<impl Responder, ApiError> {
    // Get the latest finalized checkpoint
    let finalized_checkpoint = db.finalized_checkpoint_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to get finalized checkpoint, error: {err:?}"
        ))
    })?;

    // Get the latest head block root from the latest slot
    let latest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get latest slot, error: {err:?}"))
        })?
        .ok_or_else(|| ApiError::NotFound("Light client finality update unavailable".into()))?;

    let head_block_root = db
        .slot_index_provider()
        .get(latest_slot)
        .map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to get block root for latest slot, error: {err:?}"
            ))
        })?
        .ok_or_else(|| ApiError::NotFound("Light client finality update unavailable".into()))?;

    // Get the head block and state
    let head_block = db
        .beacon_block_provider()
        .get(head_block_root)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get head block, error: {err:?}"))
        })?
        .ok_or_else(|| ApiError::NotFound("Light client finality update unavailable".into()))?;

    // Get the attested block (parent of head block) and its state
    let attested_block = db
        .beacon_block_provider()
        .get(head_block.message.parent_root)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get attested block, error: {err:?}"))
        })?
        .ok_or_else(|| ApiError::NotFound("Light client finality update unavailable".into()))?;

    let attested_block_root = attested_block.message.tree_hash_root();
    let attested_state = db
        .beacon_state_provider()
        .get(attested_block_root)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get attested state, error: {err:?}"))
        })?
        .ok_or_else(|| ApiError::NotFound("Light client finality update unavailable".into()))?;

    // Get the finalized block
    let finalized_block = db
        .beacon_block_provider()
        .get(finalized_checkpoint.root)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get finalized block, error: {err:?}"))
        })?
        .ok_or_else(|| ApiError::NotFound("Light client finality update unavailable".into()))?;

    // Create the finality update
    let attested_header = LightClientHeader::new(&attested_block).map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to create attested light client header: {err:?}"
        ))
    })?;
    let finalized_header = LightClientHeader::new(&finalized_block).map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to create finalized light client header: {err:?}"
        ))
    })?;
    let finality_update = LightClientFinalityUpdate {
        attested_header,
        finalized_header,
        finality_branch: attested_state
            .finalized_root_inclusion_proof()
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get finalized root inclusion proof, error: {err:?}"
                ))
            })?
            .into(),
        sync_aggregate: head_block.message.body.sync_aggregate,
        signature_slot: head_block.message.slot,
    };

    // Check Accept header for response format
    let response = match http_request
        .headers()
        .get("accept")
        .and_then(|header| header.to_str().ok())
    {
        Some(SSZ_CONTENT_TYPE) => HttpResponse::Ok()
            .content_type(SSZ_CONTENT_TYPE)
            .insert_header((ETH_CONSENSUS_VERSION_HEADER, VERSION))
            .body(finality_update.as_ssz_bytes()),
        _ => HttpResponse::Ok()
            .content_type(JSON_CONTENT_TYPE)
            .insert_header((ETH_CONSENSUS_VERSION_HEADER, VERSION))
            .json(DataVersionedResponse::new(finality_update)),
    };

    Ok(response)
}
