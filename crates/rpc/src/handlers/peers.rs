use std::{str::FromStr, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use libp2p::PeerId;
use ream_p2p::network_state::NetworkState;

use crate::types::{errors::ApiError, response::DataResponse};

/// GET /eth/v1/node/peers/{peer_id}
#[get("/node/peers/{peer_id}")]
pub async fn get_peer(
    network_state: Data<Arc<NetworkState>>,
    peer_id: Path<String>,
) -> Result<impl Responder, ApiError> {
    let peer_id = peer_id.into_inner();
    let peer_id = PeerId::from_str(&peer_id).map_err(|err| {
        ApiError::BadRequest(format!("Invalid PeerId format: {peer_id}, {err:?}"))
    })?;

    let cached_peer = network_state
        .peer_table
        .read()
        .get(&peer_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("Peer not found: {peer_id}")))?;

    Ok(HttpResponse::Ok().json(DataResponse::new(&cached_peer)))
}
