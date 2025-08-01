use alloy_primitives::B256;
use ream_pqc::PQSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedVote {
    pub data: Vote,
    pub signature: PQSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Vote {
    pub validator_id: u64,
    pub slot: u64,
    pub head: B256,
    pub head_slot: u64,
    pub target: B256,
    pub target_slot: u64,
    pub source: B256,
    pub source_slot: u64,
}
