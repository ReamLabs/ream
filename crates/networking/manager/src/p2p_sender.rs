use anyhow::anyhow;
use libp2p::{PeerId, swarm::ConnectionId};
use ream_p2p::{
    channel::{GossipMessage, P2PMessage, P2PResponse},
    req_resp::{error::ReqRespError, handler::RespMessage, messages::ResponseMessage},
};
use tokio::sync::mpsc;
use tracing::warn;

pub struct P2PSender(pub mpsc::UnboundedSender<P2PMessage>);

impl P2PSender {
    pub fn send_gossip(&self, message: GossipMessage) {
        if let Err(err) = self.0.send(P2PMessage::Gossip(message)) {
            warn!("Failed to send gossip message: {err}");
        }
    }

    pub fn send_response(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
        message: ResponseMessage,
    ) {
        if let Err(err) = self.0.send(P2PMessage::Response(P2PResponse {
            peer_id,
            connection_id,
            stream_id,
            message: RespMessage::Response(Box::new(message)),
        })) {
            warn!("Failed to send P2P response: {err}");
        }
    }

    pub fn send_end_of_stream_response(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
    ) {
        if let Err(err) = self.0.send(P2PMessage::Response(P2PResponse {
            peer_id,
            connection_id,
            stream_id,
            message: RespMessage::EndOfStream,
        })) {
            warn!("Failed to send end of stream response: {err}");
        }
    }

    pub fn send_error_response(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
        error: &str,
    ) {
        if let Err(err) = self.0.send(P2PMessage::Response(P2PResponse {
            peer_id,
            connection_id,
            stream_id,
            message: RespMessage::Error(ReqRespError::Anyhow(anyhow!(error.to_string()))),
        })) {
            warn!("Failed to send error response: {err}");
        }
    }
}
