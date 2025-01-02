use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::{pubkey::PubKey, signature::BlsSignature};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct DepositData {
    pub pubkey: PubKey,
    pub withdrawal_credentials: B256,
    pub amount: u64,

    /// BLS aggregate signature
    pub signature: BlsSignature,
}