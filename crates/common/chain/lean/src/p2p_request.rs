use std::sync::Arc;

use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(Arc<SignedBlock>),
    GossipVote(Arc<SignedVote>),
}
