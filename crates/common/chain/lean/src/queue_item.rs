use std::sync::Arc;

use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum QueueItem {
    Block(Arc<SignedBlock>),
    SignedVote(Arc<SignedVote>),
}
