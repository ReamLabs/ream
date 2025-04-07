use ream_storage::db::ReamDB;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::{
    BeaconResponse,
    state::{get_state_from_id, get_state_root},
};
use crate::{handlers::RootResponse, types::id::ID};

pub async fn get_root(state_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;

    let state_root = get_state_root(&state);

    Ok(with_status(
        BeaconResponse::json(RootResponse::new(state_root)),
        StatusCode::OK,
    ))
}
