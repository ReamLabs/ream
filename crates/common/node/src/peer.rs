use libp2p_identity::PeerId;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PeerCountData {
    #[serde(with = "serde_utils::quoted_u64")]
    pub disconnected: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub connecting: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub connected: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub disconnecting: u64,
}

pub struct Peer {
    pub id: PeerId,
    pub status: PeerStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerStatus {
    Connected,
    Connecting,
    Disconnecting,
    Disconnected,
}
