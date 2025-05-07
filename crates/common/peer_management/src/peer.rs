use std::time::{Instant, SystemTime};

use libp2p::{PeerId, core::multiaddr::Multiaddr};

use crate::score::ReputationScore;

/// Status of a peer connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerConnectionStatus {
    /// We are connected to this peer.
    Connected,
    /// We are currently connecting to this peer.
    Connecting,
    /// We are disconnected from this peer.
    Disconnected,
    /// This peer has been banned.
    Banned { until: SystemTime },
    /// The connection to this peer is being dropped.
    Disconnecting,
}

/// Represents a peer in the network
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// The libp2p peer ID
    pub peer_id: PeerId,
    /// The multiaddress of the peer
    pub addr: Multiaddr,
    /// Last time we received a message from this peer
    pub last_seen: Instant,
    /// The current connection status.
    pub connection_status: PeerConnectionStatus,
    /// The peer's reputation score.
    pub reputation: ReputationScore,
}

impl PeerInfo {
    pub fn new(peer_id: PeerId, addr: Multiaddr) -> Self {
        Self {
            peer_id,
            addr,
            last_seen: Instant::now(),
            connection_status: PeerConnectionStatus::Connecting,
            reputation: ReputationScore::default(),
        }
    }

    /// Update the last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = Instant::now();
    }

    /// Check if the peer is connected
    pub fn is_connected(&self) -> bool {
        self.connection_status == PeerConnectionStatus::Connected
    }

    /// Check if the peer is banned
    pub fn is_banned(&self) -> bool {
        matches!(self.connection_status, PeerConnectionStatus::Banned { .. })
    }
}
