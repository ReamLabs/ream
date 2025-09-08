use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum QueueItem {
    Block(SignedBlock),
    SignedVote(SignedVote),
}
