use std::time::Instant;

use discv5::Enr;
use libp2p::{Multiaddr, PeerId};

use crate::{
    network::peer::{ConnectionState, Direction},
    req_resp::beacon::messages::{meta_data::GetMetaDataV2, status::Status},
};

#[derive(Clone, Debug)]
pub struct CachedPeer {
    /// libp2p peer ID
    pub peer_id: PeerId,

    /// Last known multiaddress observed for the peer
    pub last_seen_p2p_address: Option<Multiaddr>,

    /// Current known connection state
    pub state: ConnectionState,

    /// Direction of the most recent connection (inbound/outbound)
    pub direction: Direction,

    /// Last time we received a message from this peer
    pub last_seen: Instant,

    /// Ethereum Node Record (ENR), if known
    pub enr: Option<Enr>,

    pub status: Option<Status>,

    pub meta_data: Option<GetMetaDataV2>,
}

impl CachedPeer {
    pub fn new(
        peer_id: PeerId,
        address: Option<Multiaddr>,
        state: ConnectionState,
        direction: Direction,
        enr: Option<Enr>,
    ) -> Self {
        CachedPeer {
            peer_id,
            last_seen_p2p_address: address,
            state,
            direction,
            last_seen: Instant::now(),
            enr,
            status: None,
            meta_data: None,
        }
    }

    /// Update the last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }
}
