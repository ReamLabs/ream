use alloy_primitives::B256;
use libp2p::{PeerId, swarm::ConnectionId};
use ream_consensus_beacon::blob_sidecar::BlobIdentifier;
use tokio::sync::mpsc;

use crate::{
    gossipsub::topics::GossipTopic,
    req_resp::{
        handler::RespMessage,
        messages::{ResponseMessage, status::Status},
    },
};

pub enum P2PCallbackResponse {
    ResponseMessage(Box<ResponseMessage>),
    Disconnected,
    Timeout,
    EndOfStream,
}

pub enum P2PMessage {
    Request(P2PRequest),
    Response(P2PResponse),
    Gossip(GossipMessage),
}

pub enum P2PRequest {
    Status {
        peer_id: PeerId,
        status: Status,
    },
    BlockRange {
        peer_id: PeerId,
        start: u64,
        count: u64,
        callback: mpsc::Sender<anyhow::Result<P2PCallbackResponse>>,
    },
    BlockRoots {
        peer_id: PeerId,
        roots: Vec<B256>,
        callback: mpsc::Sender<anyhow::Result<P2PCallbackResponse>>,
    },
    BlobIdentifiers {
        peer_id: PeerId,
        blob_identifiers: Vec<BlobIdentifier>,
        callback: mpsc::Sender<anyhow::Result<P2PCallbackResponse>>,
    },
}

pub struct P2PResponse {
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub stream_id: u64,
    pub message: Box<RespMessage>,
}

#[derive(Debug, Clone)]
pub struct GossipMessage {
    pub topic: GossipTopic,
    pub data: Vec<u8>,
}
