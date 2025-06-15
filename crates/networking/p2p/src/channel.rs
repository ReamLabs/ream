use libp2p::{PeerId, swarm::ConnectionId};
use tokio::sync::mpsc;

use crate::req_resp::{
    handler::RespMessage,
    messages::{ResponseMessage, status::Status},
};

pub enum P2PCallbackResponse {
    ResponseMessage(Box<ResponseMessage>),
    EndOfStream,
}

pub enum P2PMessage {
    Request(P2PRequest),
    Response(P2PResponse),
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
}

pub struct P2PResponse {
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub stream_id: u64,
    pub message: RespMessage,
}
