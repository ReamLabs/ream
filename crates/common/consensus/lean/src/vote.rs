use ethereum_hashing::hash;
use ream_pqc::PQSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::Hash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedVote {
    pub data: Vote,
    pub signature: PQSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Vote {
    pub validator_id: usize,
    pub slot: usize,
    pub head: Hash,
    pub head_slot: usize,
    pub target: Hash,
    pub target_slot: usize,
    pub source: Hash,
    pub source_slot: usize,
}

impl Vote {
    pub fn compute_hash(&self) -> Hash {
        let serialized = serde_json::to_string(self).unwrap();
        Hash::from_slice(&hash(serialized.as_bytes()))
    }
}
