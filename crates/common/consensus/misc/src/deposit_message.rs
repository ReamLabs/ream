use alloy_primitives::B256;
use ream_bls::PublicKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct DepositMessage {
    #[serde(rename = "pubkey")]
    pub public_key: PublicKey,
    pub withdrawal_credentials: B256,
    pub amount: u64,
}
