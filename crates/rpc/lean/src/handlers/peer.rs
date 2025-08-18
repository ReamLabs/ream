use std::{collections::HashMap, sync::Arc};

use actix_web::{HttpResponse, Responder, get, web::Data};
use libp2p::PeerId;
use parking_lot::Mutex;
use ream_api_types_beacon::error::ApiError;
use ream_p2p::network::peer::ConnectionState;
use ream_rpc_beacon::handlers::peers::PeerCount;

// /lean/v0/node/peers
#[get("/node/peers")]
pub async fn list_peers(
    peer_table: Data<Arc<Mutex<HashMap<PeerId, ConnectionState>>>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(peer_table.lock().clone()))
}

// /lean/v0/node/peer_count
#[get("/node/peer_count")]
pub async fn get_peer_count(
    peer_table: Data<Arc<Mutex<HashMap<PeerId, ConnectionState>>>>,
) -> Result<impl Responder, ApiError> {
    let mut peer_count = PeerCount::default();

    for connection_state in peer_table.lock().values() {
        match connection_state {
            ConnectionState::Connected => peer_count.connected += 1,
            ConnectionState::Connecting => peer_count.connecting += 1,
            ConnectionState::Disconnected => peer_count.disconnected += 1,
            ConnectionState::Disconnecting => peer_count.disconnecting += 1,
        }
    }

    Ok(HttpResponse::Ok().json(&peer_count))
}
