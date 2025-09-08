use ream_consensus_lean::{block::Block, vote::SignedVote};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum QueueItem {
    Block(Block),
    SignedVote(Box<SignedVote>),
}
