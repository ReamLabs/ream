use ream_consensus_lean::checkpoint::Checkpoint;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Encode, Decode, Serialize, Deserialize)]
pub struct Status {
    /// The client's latest finalized checkpoint
    pub finalized: Checkpoint,

    /// The client's current head checkpoint
    pub head: Checkpoint,
}
