use ream_consensus_lean::{block::SignedBlock, vote::SignedAttestation};

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(SignedBlock),
    GossipVote(SignedAttestation),
}
