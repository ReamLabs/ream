use alloy_primitives::B256;
use ethereum_hashing::hash;
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
    pub validator_id: usize,
    pub slot: usize,
    pub head: B256,
    pub head_slot: usize,
    pub target: B256,
    pub target_slot: usize,
    pub source: B256,
    pub source_slot: usize,
}

impl Vote {
    pub fn compute_hash(&self) -> B256 {
        let serialized = serde_json::to_string(self).unwrap();
        B256::from_slice(&hash(serialized.as_bytes()))
    }
}
