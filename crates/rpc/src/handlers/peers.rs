use std::str::FromStr;

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use libp2p::PeerId;
use ream_p2p::network::{CachedPeer, PeerTable};
use serde::{Deserialize, Serialize};

use crate::types::{errors::ApiError, response::DataResponse};

#[derive(Debug, Serialize, Deserialize)]
pub struct PeerData {
    peer_id: String,
    enr: Option<String>,
    last_seen_p2p_address: String,
    state: String,
    direction: String,
}

impl From<&CachedPeer> for PeerData {
    fn from(p: &CachedPeer) -> Self {
        Self {
            peer_id: p.peer_id.to_string(),
            enr: p.enr.as_ref().map(|e| e.to_base64()),
            last_seen_p2p_address: p
                .last_seen_p2p_address
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_default(),
            state: p.state.to_string(),
            direction: p.direction.to_string(),
        }
    }
}

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

    Ok(HttpResponse::Ok().json(DataResponse::new(PeerData::from(&snap))))
}
