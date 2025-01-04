use alloy_primitives::{aliases::B32, B256};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ForkData {
    pub current_version: B32,
    pub genesis_validators_root: B256,
}
