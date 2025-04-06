use ream_storage::db::ReamDB;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::{BeaconResponse, state::get_state_from_id};
use crate::types::id::ID;

pub async fn get_root(state_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;
    Ok(with_status(
        BeaconResponse::json(state.root),
        StatusCode::OK,
    ))
}

