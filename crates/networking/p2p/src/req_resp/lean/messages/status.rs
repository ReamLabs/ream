use ream_consensus_lean::checkpoint::Checkpoint;
use ssz_derive::{Decode, Encode};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LeanStatus {
    /// The client's latest finalized checkpoint
    pub finalized: Checkpoint,

    /// The client's current head checkpoint
    pub head: Checkpoint,
}
