use alloy_primitives::B256;
use ream_pqc::PQSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBlock {
    pub message: Block,
    pub signature: PQSignature,
}

#[derive(
    Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Default,
)]
pub struct Block {
    pub slot: u64,
    pub proposer_index: u64,

    /// Empty root
    pub body: B256,
}
