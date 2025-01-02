use alloy_primitives::aliases::B32;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Fork {
    pub previous_version: B32,
    pub current_version: B32,
    pub epoch: u64,
}
