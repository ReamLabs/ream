use alloy_primitives::FixedBytes;
use ream_consensus::deneb::beacon_state::BeaconState;
use ream_storage::{db::ReamDB, tables::{ Field, Table}};
use warp::{
    http::status::StatusCode, 
    reject::Rejection, 
    reply::{with_status, Reply}
};

use crate::types::{errors::ApiError, id::ID};

use super::BeaconResponse;

pub async fn get_state_from_id(state_id: ID, db: &ReamDB) -> Result<BeaconState, ApiError> {
    let block_root = match state_id {
        ID::Named(ref name) => match name.as_str() {
            "head" => {Ok(Some(FixedBytes::new([0; 32])))},
            "justified" => {
                let justified_checkpoint = db.justified_checkpoint_provider().get().map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| ApiError::NotFound(String::from("Justified checkpoint not found")))?;
                
                Ok(Some(justified_checkpoint.root))
            },
            "finalized" => {
                let finalized_checkpoint = db.finalized_checkpoint_provider().get().map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| ApiError::NotFound(String::from("Finalized checkpoint not found")))?;
                
                Ok(Some(finalized_checkpoint.root))
            },
            "genesis" => db.slot_index_provider().get(0),
            &_ => Ok(Some(FixedBytes::new([0; 32])))
        },
        ID::Slot(slot) => db.slot_index_provider().get(slot),
        ID::Root(root) => db.state_root_index_provider().get(root),
    }
    .map_err(|_| ApiError::InternalError)?
    .ok_or(ApiError::NotFound(format!(
        "Failed to find `block_root` from {state_id:?}"
    )))?;

    db.beacon_state_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon_state` from {block_root:?}"
        )))
}

pub async fn get_state(state_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;

    Ok(with_status(BeaconResponse::json(state), StatusCode::OK))
}