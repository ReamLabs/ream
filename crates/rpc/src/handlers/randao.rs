use alloy_primitives::FixedBytes;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::{BeaconResponse, state::get_state_from_id};
use crate::types::id::ID;

#[derive(Serialize, Deserialize)]
struct RandaoResponse {
    pub randao: FixedBytes<32>,
}

/// Called by `/states/<state_id>/randao` to get the Randao mix of state.
/// Pass optional `epoch` in the query to get randao for particular epoch,
/// else will fetch randao of the state epoch
pub async fn get_randao_mix(
    state_id: ID,
    epoch: Option<u64>,
    db: ReamDB,
) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;
    let randao_mix = match epoch {
        Some(epoch) => state.get_randao_mix(epoch),
        None => state.get_randao_mix(state.get_current_epoch()),
    };

    Ok(with_status(
        BeaconResponse::json(RandaoResponse { randao: randao_mix }),
        StatusCode::OK,
    ))
}
