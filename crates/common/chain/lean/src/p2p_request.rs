use alloy_primitives::B256;
use libp2p_identity::PeerId;
use libp2p_swarm::ConnectionId;
use ream_consensus_lean::{
    attestation::{SignedAggregatedAttestation, SignedAttestation},
    block::SignedBlock,
};
use ream_req_resp::lean::{ResponseCallback, messages::LeanResponseMessage};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(Box<SignedBlock>),
    GossipAttestation {
        subnet_id: u64,
        attestation: Box<SignedAttestation>,
    },
    GossipAggregatedAttestation(Box<SignedAggregatedAttestation>),
    Request {
        peer_id: PeerId,
        callback: mpsc::Sender<ResponseCallback>,
        message: P2PCallbackRequest,
    },
    Response {
        peer_id: PeerId,
        stream_id: u64,
        connection_id: ConnectionId,
        message: LeanResponseMessage,
    },
    EndOfStream {
        peer_id: PeerId,
        stream_id: u64,
        connection_id: ConnectionId,
    },
}

#[derive(Debug, Clone)]
pub enum P2PCallbackRequest {
    BlocksByRoot { roots: Vec<B256> },
    Status,
}
