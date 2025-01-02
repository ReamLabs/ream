use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::pubkey::PubKey;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    pub pubkey: PubKey,

    /// Commitment to pubkey for withdrawals
    pub withdrawal_credentials: B256,

    /// Balance at stake
    pub effective_balance: u64,
    pub slashed: bool,

    /// When criteria for activation were met
    pub activation_eligibility_epoch: u64,
    pub activation_epoch: u64,
    pub exit_epoch: u64,

    /// When validator can withdraw funds
    pub withdrawable_epoch: u64,
}
