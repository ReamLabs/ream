use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(SignedBlock),
    GossipVote(SignedVote),
}
