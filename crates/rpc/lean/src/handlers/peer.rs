use std::sync::Arc;

use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_api_types_common::error::ApiError;
use ream_network_state_lean::NetworkState;
use ream_peer::{ConnectionState, PeerCount};

// /lean/v0/node/peers
#[get("/node/peers")]
pub async fn list_peers(
    network_state: Data<Arc<NetworkState>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(network_state.peer_table.lock().clone()))
}

// /lean/v0/node/peer_count
#[get("/node/peer_count")]
pub async fn get_peer_count(
    network_state: Data<Arc<NetworkState>>,
) -> Result<impl Responder, ApiError> {
    let mut peer_count = PeerCount::default();

    for connection_state in network_state
        .peer_table
        .lock()
        .values()
        .map(|peer| peer.state)
    {
        match connection_state {
            ConnectionState::Connected => peer_count.connected += 1,
            ConnectionState::Connecting => peer_count.connecting += 1,
            ConnectionState::Disconnected => peer_count.disconnected += 1,
            ConnectionState::Disconnecting => peer_count.disconnecting += 1,
        }
    }

    Ok(HttpResponse::Ok().json(&peer_count))
}
