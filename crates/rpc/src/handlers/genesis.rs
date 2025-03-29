use std::sync::Arc;

use serde_json::json;

use crate::types::genesis::Genesis;

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
pub async fn get_genesis(ctx: Arc<Genesis>) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(warp::reply::with_status(
        warp::reply::json(
            &json!({"genesis_time":ctx.genesis_time.to_string(),"genesis_validator_root":ctx.genesis_validator_root.to_string(),"genesis_fork_version":ctx.genesis_fork_version.to_string()}),
        ),
        warp::http::status::StatusCode::OK,
    ))
}
