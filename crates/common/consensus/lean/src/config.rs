use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Config {
    pub num_validators: u64,
}
