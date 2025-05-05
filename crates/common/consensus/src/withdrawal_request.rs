use alloy_primitives::Address;
use ream_bls::PubKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::misc::checksummed_address;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct WithdrawalRequest {
    #[serde(with = "checksummed_address")]
    pub source_address: Address,
    pub validator_pubkey: PubKey,
    #[serde(with = "serde_utils::quoted_u64")]
    pub amount: u64,
}
