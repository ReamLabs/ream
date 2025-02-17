use alloy_primitives::Address;
use alloy_rlp::RlpEncodable;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(
    Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, RlpEncodable,
)]
pub struct Withdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64,
}
