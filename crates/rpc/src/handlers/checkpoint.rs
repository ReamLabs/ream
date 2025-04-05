use ream_consensus::checkpoint::Checkpoint;
use ream_storage::db::ReamDB;
use serde::Serialize;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::{BeaconResponse, state::get_state_from_id};
use crate::types::id::ID;

#[derive(Debug, Serialize, Clone)]
pub struct CheckpointData {
    previous_justified: Checkpoint,
    current_justified: Checkpoint,
    finalized: Checkpoint,
}

impl CheckpointData {
    pub fn new(
        previous_justified: Checkpoint,
        current_justified: Checkpoint,
        finalized: Checkpoint,
    ) -> Self {
        Self {
            previous_justified,
            current_justified,
            finalized,
        }
    }
}

/// Called by `/states/<state_id>/finality_checkpoints` to get the Checkpoint Data of state.
pub async fn get_finality_checkpoint(state_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;
    Ok(with_status(
        BeaconResponse::json(CheckpointData::new(
            state.previous_justified_checkpoint,
            state.current_justified_checkpoint,
            state.finalized_checkpoint,
        )),
        StatusCode::OK,
    ))
}
