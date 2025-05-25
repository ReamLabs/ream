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
    table: Data<PeerTable>,
    peer_id: Path<String>,
) -> Result<impl Responder, ApiError> {
    let id_raw = peer_id.into_inner();
    let peer_id = PeerId::from_str(&id_raw)
        .map_err(|_| ApiError::BadRequest(format!("Invalid peer ID: {id_raw}")))?;

    let snap = table
        .read()
        .get(&peer_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("Peer not found: {id_raw}")))?;

    Ok(HttpResponse::Ok().json(DataResponse::new(&snap)))
}
