use ream_consensus::genesis::Genesis;
use serde_json::json;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, json, with_status},
};

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
pub async fn get_genesis(genesis: Genesis) -> Result<impl Reply, Rejection> {
    Ok(with_status(json(&json!(genesis)), StatusCode::OK))
}
