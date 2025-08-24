use ream_post_quantum_crypto::PQSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::checkpoint::Checkpoint;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedVote {
    pub data: Vote,
    pub signature: PQSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Vote {
    pub validator_id: u64,
    pub slot: u64,
    pub head: Checkpoint,
    pub target: Checkpoint,
    pub source: Checkpoint,
}
