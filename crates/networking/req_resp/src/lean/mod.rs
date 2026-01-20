pub mod messages;
pub mod protocol_id;

use std::sync::Arc;

use libp2p::PeerId;

use crate::{
    ConnectionId,
    handler::ReqRespMessageError,
    lean::messages::{LeanRequestMessage, LeanResponseMessage},
};

#[derive(Debug)]
pub enum ReamNetworkEvent {
    Event(NetworkEvent),
    ResponseCallback(ResponseCallback),
}

#[derive(Debug)]
pub enum NetworkEvent {
    RequestMessage {
        peer_id: PeerId,
        stream_id: u64,
        connection_id: ConnectionId,
        message: LeanRequestMessage,
    },
    NetworkError {
        peer_id: PeerId,
        error: ReqRespMessageError,
    },
}

#[derive(Debug)]
pub enum ResponseCallback {
    ResponseMessage {
        peer_id: PeerId,
        request_id: u64,
        message: Arc<LeanResponseMessage>,
    },
    EndOfStream {
        peer_id: PeerId,
        request_id: u64,
    },
    NotConnected {
        peer_id: PeerId,
    },
}
