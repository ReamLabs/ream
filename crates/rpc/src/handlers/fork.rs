use ream_storage::db::ReamDB;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::state::get_state_from_id;
use crate::types::{id::ID, response::BeaconResponse};

/// Called by `/eth/v1/beacon/states/{state_id}/fork` to get fork of state.
pub async fn get_fork(state_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;
    Ok(with_status(
        BeaconResponse::json(state.fork),
        StatusCode::OK,
    ))
}
