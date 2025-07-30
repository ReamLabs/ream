use ream_pqc::PQSignature;
use ethereum_hashing::hash;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::Hash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Vote {
    pub data: VoteData,
    pub signature: PQSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct VoteData {
    pub validator_id: u64,
    pub slot: u64,
    pub head: Hash,
    pub head_slot: u64,
    pub target: Hash,
    pub target_slot: u64,
    pub source: Hash,
    pub source_slot: u64,
}

impl Vote {
    pub fn compute_hash(&self) -> Hash {
        let serialized = serde_json::to_string(self).unwrap();
        Hash::from_slice(&hash(serialized.as_bytes()))
    }
}
