use ream_consensus::deneb::beacon_state::BeaconState;
use ream_storage::{db::ReamDB, tables::Table};

use crate::types::{errors::ApiError, id::ID};

pub async fn get_state_from_id(state_id: ID, db: &ReamDB) -> Result<BeaconState, ApiError> {
    let block_root = match state_id {
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
