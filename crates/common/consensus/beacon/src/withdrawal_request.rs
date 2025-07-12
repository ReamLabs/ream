use alloy_primitives::Address;
use ream_bls::PublicKey;
use ream_consensus_misc::misc::checksummed_address;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct WithdrawalRequest {
    #[serde(with = "checksummed_address")]
    pub source_address: Address,
    #[serde(rename = "validator_pubkey")]
    pub validator_public_key: PublicKey,
    #[serde(with = "serde_utils::quoted_u64")]
    pub amount: u64,
}
