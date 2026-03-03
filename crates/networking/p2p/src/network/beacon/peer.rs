use std::time::Instant;

use discv5::Enr;
use libp2p::{Multiaddr, PeerId};
use ream_peer::{ConnectionState, Direction};
use ream_req_resp::beacon::messages::{meta_data::GetMetaDataV3, status::Status};

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

    pub meta_data: Option<GetMetaDataV3>,

    /// DAS peer sampling score. Tracks responsiveness for data availability sampling.
    /// Initialized to a neutral midpoint (128). Successful sampling responses increase
    /// the score; failures decrease it more aggressively to penalize unreliable peers.
    pub sampling_score: u8,

    /// Total number of DAS sampling requests sent to this peer.
    pub sampling_requests: u64,

    /// Total number of successful DAS sampling responses from this peer.
    pub sampling_successes: u64,

    /// Total number of failed DAS sampling responses from this peer.
    pub sampling_failures: u64,
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
            sampling_score: u8::MAX / 2,
            sampling_requests: 0,
            sampling_successes: 0,
            sampling_failures: 0,
        }
    }

    /// Update the last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }
}
