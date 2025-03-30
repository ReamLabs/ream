use ream_execution_engine::rpc_types::genesis::Genesis;
use serde_json::json;
use warp::{
    http::status::StatusCode,
    reply::{json, with_status},
};

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
pub async fn get_genesis(context: Genesis) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(with_status(
        json(
            &json!({"genesis_time":context.genesis_time.to_string(),"genesis_validator_root":context.genesis_validator_root.to_string(),"genesis_fork_version":context.genesis_fork_version.to_string()}),
        ),
        StatusCode::OK,
    ))
}
