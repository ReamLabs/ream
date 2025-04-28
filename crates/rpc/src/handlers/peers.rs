use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::types::{errors::ApiError, response::DataResponse};

#[derive(Debug, Serialize, Deserialize)]
pub struct PeerData {
    peer_id: String,
    enr: Option<String>,
    last_seen_p2p_address: String,
    state: String,
    direction: String,
}

impl PeerData {
    pub fn new(
        peer_id: String,
        enr: Option<String>,
        last_seen_p2p_address: String,
        state: String,
        direction: String,
    ) -> Self {
        Self {
            peer_id,
            enr,
            last_seen_p2p_address,
            state,
            direction,
        }
    }
}

/// Called by `/eth/v1/node/peers/{peer_id}` to return the current connection
#[get("/node/peers/{peer_id}")]
pub async fn get_peer(db: Data<ReamDB>, peer_id: Path<String>) -> Result<impl Responder, ApiError> {
    let peer_id = peer_id.into_inner();
    let (enr, last_seen, state, direction) = match mock_fetch_peer(&peer_id, &db).await {
        Ok(tuple) => tuple,
        Err(ApiError::NotFound(_)) => {
            return Err(ApiError::NotFound(format!("Peer not found: {peer_id}")));
        }
        Err(e) => {
            error!("Failed to fetch peer {peer_id}, err: {e:?}");
            return Err(ApiError::InternalError);
        }
    };

    let payload = PeerData::new(peer_id, enr, last_seen, state, direction);
    Ok(HttpResponse::Ok().json(DataResponse::new(payload)))
}

async fn mock_fetch_peer(
    peer_id: &str,
    _db: &ReamDB,
) -> Result<(Option<String>, String, String, String), ApiError> {
    if peer_id != "QmMockPeer_______" {
        return Err(ApiError::NotFound("peer".into()));
    }

    Ok((
        None,
        "/ip4/127.0.0.1/tcp/9000/p2p/QmMockPeer".into(),
        "connected".into(),
        "outbound".into(),
    ))
}
