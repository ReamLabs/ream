use std::{str::FromStr, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use discv5::Enr;
use libp2p::{Multiaddr, PeerId};
use ream_api_types_beacon::{error::ApiError, responses::DataResponse};
use ream_p2p::network::{
    beacon::network_state::NetworkState,
    peer::{ConnectionState, Direction},
};
use serde::Serialize;

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

    Ok(HttpResponse::Ok().json(DataResponse::new(&Peer {
        peer_id: cached_peer.peer_id,
        last_seen_p2p_address: cached_peer.last_seen_p2p_address,
        state: cached_peer.state,
        direction: cached_peer.direction,
        enr: cached_peer.enr,
    })))
}

#[get("/node/peer_count")]
pub async fn get_peer_count(
    network_state: Data<Arc<NetworkState>>,
) -> Result<impl Responder, ApiError> {
    let mut peer_count = PeerCount::default();
    for peer in network_state.peer_table.read().values() {
        match peer.state {
            ConnectionState::Connected => peer_count.connected += 1,
            ConnectionState::Connecting => peer_count.connecting += 1,
            ConnectionState::Disconnected => peer_count.disconnected += 1,
            ConnectionState::Disconnecting => peer_count.disconnecting += 1,
        }
    }
    Ok(HttpResponse::Ok().json(DataResponse::new(peer_count)))
}

#[derive(Default, Debug, Clone, Serialize)]
pub struct PeerCount {
    #[serde(with = "serde_utils::quoted_u64")]
    pub disconnected: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub connecting: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub connected: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub disconnecting: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Peer {
    /// libp2p peer ID
    pub peer_id: PeerId,

    /// Last known multiaddress observed for the peer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_p2p_address: Option<Multiaddr>,

    /// Current known connection state
    pub state: ConnectionState,

    /// Direction of the most recent connection (inbound/outbound)
    pub direction: Direction,

    /// Ethereum Node Record (ENR), if known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enr: Option<Enr>,
}
