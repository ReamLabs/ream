use alloy_primitives::B256;
use ream_bls::{BLSSignature, PubKey};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingDeposit {
    pub pubkey: PubKey,
    pub withdrawal_credentials: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub amount: u64,
    pub signature: BLSSignature,
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
}
