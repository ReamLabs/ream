use std::{str::FromStr, sync::Arc};

use alloy_primitives::B256;
use ream_consensus::deneb::beacon_state::BeaconState;
use ream_storage::{
    db::ReamDB,
    tables::{Table, beacon_state::BeaconStateTable, state_root_index::StateRootIndexTable},
};

use crate::types::errors::ApiError;

pub async fn get_state_from_id(state_id: String, db: Arc<ReamDB>) -> Result<BeaconState, ApiError> {
    let state_root = match B256::from_str(&state_id) {
        Ok(value) => value,
        Err(_) => {
            return Err(ApiError::BadRequest(state_id));
        }
    };

    let state_root_index_table = StateRootIndexTable { db: db.db.clone() };

    match state_root_index_table.get(state_root) {
        // received block root
        Ok(Some(block_root)) => {
            let beacon_state_table = BeaconStateTable { db: db.db.clone() };

            let state = match beacon_state_table.get(block_root) {
                Ok(Some(state)) => state,
                Ok(None) => return Err(ApiError::NotFound(state_id)),
                Err(_) => return Err(ApiError::InternalError),
            };

            Ok(state)
        }
        // no block root
        Ok(None) => Err(ApiError::NotFound(state_id)),

        // unable to fetch
        Err(_) => Err(ApiError::InternalError),
    }
}
