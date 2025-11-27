use std::{str::FromStr, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path, Query},
};
use discv5::Enr;
use libp2p::{Multiaddr, PeerId};
use ream_api_types_beacon::{
    query::{ConnectionStateQuery, DirectionQuery},
    responses::{DataResponse, DataResponseWithMeta},
};
use ream_api_types_common::error::ApiError;
use ream_p2p::network::beacon::network_state::NetworkState;
use ream_peer::{ConnectionState, Direction, PeerCount, PeersMetadata};
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

/// GET /eth/v1/node/peers
#[get("/node/peers")]
pub async fn get_peers(
    network_state: Data<Arc<NetworkState>>,
    state: Query<ConnectionStateQuery>,
    direction: Query<DirectionQuery>,
) -> Result<impl Responder, ApiError> {
    let peer_table = network_state.peer_table.read();

    let peers: Vec<Peer> = peer_table
        .values()
        .filter(|cached_peer| {
            // Filter by state if provided
            if let Some(ref states) = state.state
                && !states.contains(&cached_peer.state)
            {
                return false;
            }

            // Filter by direction if provided
            if let Some(ref directions) = direction.direction {
                // Unknown direction doesn't match any filter (not in spec)
                if cached_peer.direction == Direction::Unknown {
                    return false;
                }
                if !directions.contains(&cached_peer.direction) {
                    return false;
                }
            }

            true
        })
        .map(|cached_peer| Peer {
            peer_id: cached_peer.peer_id,
            enr: cached_peer.enr.clone(),
            last_seen_p2p_address: cached_peer.last_seen_p2p_address.clone(),
            state: cached_peer.state,
            direction: cached_peer.direction,
        })
        .collect();

    let count = peers.len() as u64;

    Ok(HttpResponse::Ok().json(DataResponseWithMeta::new(peers, PeersMetadata { count })))
}

#[derive(Clone, Debug, Serialize)]
pub struct Peer {
    /// libp2p peer ID
    pub peer_id: PeerId,

    /// Ethereum Node Record (ENR), if known
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enr: Option<Enr>,

    /// Last known multiaddress observed for the peer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_p2p_address: Option<Multiaddr>,

    /// Current known connection state
    pub state: ConnectionState,

    /// Direction of the most recent connection (inbound/outbound)
    pub direction: Direction,
}
