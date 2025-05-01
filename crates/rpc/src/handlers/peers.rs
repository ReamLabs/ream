use std::str::FromStr;

use actix_web::{HttpResponse, Responder, get, web::Path};
use libp2p::PeerId;
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
pub async fn get_peer(peer_id: Path<String>) -> Result<impl Responder, ApiError> {
    let peer_id_raw = peer_id.into_inner();
    PeerId::from_str(&peer_id_raw)
        .map_err(|_| ApiError::BadRequest(format!("Invalid peer ID: {peer_id_raw}")))?;

    let (enr, last_seen, state, direction) = match mock_fetch_peer(&peer_id_raw).await {
        Ok(tuple) => tuple,
        Err(ApiError::NotFound(_)) => {
            return Err(ApiError::NotFound(format!("Peer not found: {peer_id_raw}")));
        }
        Err(e) => {
            error!("Failed to fetch peer {peer_id_raw}, err: {e:?}");
            return Err(ApiError::InternalError);
        }
    };

    let payload = PeerData::new(peer_id_raw, enr, last_seen, state, direction);
    Ok(HttpResponse::Ok().json(DataResponse::new(payload)))
}

async fn mock_fetch_peer(
    peer_id: &str,
) -> Result<(Option<String>, String, String, String), ApiError> {
    if peer_id != "16Uiu2HAm1oEch6uXffoGZ32kPTiyjycfX9yDuBJSWtmagBSk9HTN" {
        return Err(ApiError::NotFound("peer".into()));
    }

    Ok((
        None,
        "/ip4/127.0.0.1/tcp/9000/p2p/16Uiu2HAm1oEch6uXffoGZ32kPTiyjycfX9yDuBJSWtmagBSk9HTN".into(),
        "connected".into(),
        "outbound".into(),
    ))
}
