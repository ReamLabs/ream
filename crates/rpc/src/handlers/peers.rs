use std::str::FromStr;

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use libp2p::PeerId;
use ream_p2p::network::PeerTable;

use crate::types::{errors::ApiError, response::DataResponse};

/// GET /eth/v1/node/peers/{peer_id}
#[get("/node/peers/{peer_id}")]
pub async fn get_peer(
    peer_table: Data<PeerTable>,
    peer_id: Path<String>,
) -> Result<impl Responder, ApiError> {
    let raw_id: String = peer_id.into_inner();
    let peer_id = PeerId::from_str(&raw_id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid peer ID: {raw_id}")))?;

    let cached_peer = peer_table
        .read()
        .get(&peer_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("Peer not found: {raw_id}")))?;

    Ok(HttpResponse::Ok().json(DataResponse::new(&cached_peer)))
}
