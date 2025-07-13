use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Hash)]
pub struct Eth1Data {
    pub deposit_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_count: u64,
    pub block_hash: B256,
}
