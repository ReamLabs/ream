use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::{
    fork_choice::helpers::constants::{ETH1_ADDRESS_WITHDRAWAL_PREFIX, MAX_EFFECTIVE_BALANCE},
    pubkey::PubKey,
};

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

impl Validator {
    /// Check if ``validator`` has an 0x01 prefixed "eth1" withdrawal credential.
    pub fn has_eth1_withdrawal_credential(&self) -> bool {
        self.withdrawal_credentials[0..1] == ETH1_ADDRESS_WITHDRAWAL_PREFIX
    }

    /// Check if ``validator`` is fully withdrawable.
    pub fn is_fully_withdrawable_validator(&self, balance: u64, epoch: u64) -> bool {
        self.has_eth1_withdrawal_credential() && self.withdrawable_epoch <= epoch && balance > 0
    }

    /// Check if ``validator`` is partially withdrawable.
    pub fn is_partially_withdrawable_validator(&self, balance: u64) -> bool {
        self.has_eth1_withdrawal_credential()
            && self.effective_balance == MAX_EFFECTIVE_BALANCE
            && balance > MAX_EFFECTIVE_BALANCE
    }
}
