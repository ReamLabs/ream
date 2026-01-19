use alloy_primitives::B256;
use libp2p_identity::PeerId;
use libp2p_swarm::ConnectionId;
use ream_consensus_lean::{attestation::SignedAttestation, block::SignedBlockWithAttestation};
use ream_req_resp::lean::messages::LeanResponseMessage;

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(Box<SignedBlockWithAttestation>),
    GossipAttestation(Box<SignedAttestation>),
    RequestBlocksByRoot {
        peer_id: PeerId,
        roots: Vec<B256>,
    },
    RequestStatus(PeerId),
    Response {
        peer_id: PeerId,
        stream_id: u64,
        connection_id: ConnectionId,
        message: LeanResponseMessage,
    },
}
