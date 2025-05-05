use alloy_primitives::Address;
use alloy_rlp::RlpEncodable;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::misc::checksummed_address;

#[derive(
    Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, RlpEncodable,
)]
pub struct Withdrawal {
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub validator_index: u64,
    #[serde(with = "checksummed_address")]
    pub address: Address,
    #[serde(with = "serde_utils::quoted_u64")]
    pub amount: u64,
}
