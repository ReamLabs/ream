use alloy_primitives::B256;
use libp2p_identity::PeerId;
use libp2p_swarm::ConnectionId;
#[cfg(feature = "devnet3")]
use ream_consensus_lean::attestation::SignedAggregatedAttestation;
use ream_consensus_lean::{attestation::SignedAttestation, block::SignedBlockWithAttestation};
use ream_req_resp::lean::{ResponseCallback, messages::LeanResponseMessage};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(Box<SignedBlockWithAttestation>),
    #[cfg(feature = "devnet2")]
    GossipAttestation(Box<SignedAttestation>),
    #[cfg(feature = "devnet3")]
    GossipAttestation {
        subnet_id: u64,
        attestation: Box<SignedAttestation>,
    },
    #[cfg(feature = "devnet3")]
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
