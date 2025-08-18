use std::{collections::HashMap, sync::Arc};

use actix_web::{HttpResponse, Responder, get, web::Data};
use libp2p::PeerId;
use parking_lot::RwLock;
use ream_api_types_beacon::error::ApiError;
use ream_p2p::peer::ConnectionState;
use ream_rpc_beacon::handlers::peers::PeerCount;

// /lean/v0/node/peers
#[get("/node/peers")]
pub async fn list_peers(
    peer_table: Data<Arc<RwLock<HashMap<PeerId, ConnectionState>>>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(peer_table.read().clone()))
}

// /lean/v0/node/peers
#[get("/node/peer_count")]
pub async fn get_peer_count(
    peer_table: Data<Arc<RwLock<HashMap<PeerId, ConnectionState>>>>,
) -> Result<impl Responder, ApiError> {
    let mut connected = 0;
    let mut connecting = 0;
    let mut disconnected = 0;
    let mut disconnecting = 0;

    for connection_state in peer_table.read().values() {
        match connection_state {
            ConnectionState::Connected => connected += 1,
            ConnectionState::Connecting => connecting += 1,
            ConnectionState::Disconnected => disconnected += 1,
            ConnectionState::Disconnecting => disconnecting += 1,
        }
    }

    Ok(HttpResponse::Ok().json(&PeerCount {
        connected,
        connecting,
        disconnected,
        disconnecting,
    }))
}
