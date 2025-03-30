use ream_consensus::genesis::Genesis;
use serde_json::json;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, json, with_status},
};

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
pub async fn get_genesis(genesis: Genesis) -> Result<impl Reply, Rejection> {
    Ok(with_status(
        json(
            &json!({"genesis_time":genesis.genesis_time.to_string(),"genesis_validator_root":genesis.genesis_validator_root.to_string(),"genesis_fork_version":genesis.genesis_fork_version.to_string()}),
        ),
        StatusCode::OK,
    ))
}
