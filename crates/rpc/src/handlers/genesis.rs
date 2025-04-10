use ream_consensus::genesis::Genesis;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::Data;

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
pub async fn get_genesis(genesis: Genesis) -> Result<impl Reply, Rejection> {
    Ok(with_status(Data::json(genesis), StatusCode::OK))
}
