use ream_consensus::deneb::beacon_state::BeaconState;
use ream_storage::{db::ReamDB, tables::Table};

use crate::types::{errors::ApiError, id::ID};

pub async fn get_state_from_id(state_id: ID, db: ReamDB) -> Result<BeaconState, ApiError> {
    let state_root_result = match state_id {
        ID::Slot(slot) => db.slot_index_provider().get(slot),
        ID::Root(root) => db.state_root_index_provider().get(root),
    };
    match state_root_result {
        // received block root
        Ok(Some(block_root)) => {
            let state = match db.beacon_state_provider().get(block_root) {
                Ok(Some(state)) => state,
                Ok(None) => return Err(ApiError::NotFound(state_id.to_string())),
                Err(_) => return Err(ApiError::InternalError),
            };
            Ok(state)
        }
        // no block root
        Ok(None) => Err(ApiError::NotFound(state_id.to_string())),
        // unable to fetch
        Err(_) => Err(ApiError::InternalError),
    }
}
