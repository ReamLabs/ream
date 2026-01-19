use std::sync::Arc;

use libp2p::PeerId;

use crate::{
    ConnectionId,
    lean::messages::{LeanRequestMessage, LeanResponseMessage},
};

pub mod messages;
pub mod protocol_id;

#[derive(Debug)]
pub enum ReamNetworkEvent {
    RequestMessage {
        peer_id: PeerId,
        stream_id: u64,
        connection_id: ConnectionId,
        message: LeanRequestMessage,
    },
    ResponseMessage {
        peer_id: PeerId,
        message: Arc<LeanResponseMessage>,
    },
}
