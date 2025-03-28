use std::sync::Arc;

use utils::chain::BeaconChain;

use crate::{types::genesis::GenesisData, utils};

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
pub async fn get_genesis(ctx: Arc<BeaconChain>) -> Result<impl warp::Reply, warp::Rejection> {
    let genesis_data = GenesisData {
        genesis_time: ctx.genesis_time.to_string(),
        genesis_validator_root: ctx.genesis_validator_root.to_string(),
        genesis_fork_version: ctx.genesis_fork_version.to_string(),
    };

    Ok(warp::reply::with_status(
        warp::reply::json(&genesis_data),
        warp::http::status::StatusCode::OK,
    ))
}
