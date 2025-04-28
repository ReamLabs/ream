use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingConsolidation {
    #[serde(with = "serde_utils::quoted_u64")]
    pub source_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub target_index: u64,
}
