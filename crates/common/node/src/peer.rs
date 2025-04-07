#[derive(Debug)]
pub struct PeerCountResponse {
    pub data: PeerCountData,
}

#[derive(Debug)]
pub struct PeerCountData {
    pub disconnected: String,
    pub connecting: String,
    pub connected: String,
    pub disconnecting: String,
}

pub struct Peer {
    pub id: String,
    pub status: PeerStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerStatus {
    Connected,
    Connecting,
    Disconnecting,
    Disconnected,
}
