use alloy_primitives::B256;
use ream_bls::PubKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::{
    constants::{
        ETH1_ADDRESS_WITHDRAWAL_PREFIX, FAR_FUTURE_EPOCH, MAX_EFFECTIVE_BALANCE_ELECTRA,
        MIN_ACTIVATION_BALANCE,
    },
    misc::is_compounding_withdrawal_credential,
};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    pub pubkey: PubKey,

    /// Commitment to pubkey for withdrawals
    pub withdrawal_credentials: B256,

    /// Balance at stake
    #[serde(with = "serde_utils::quoted_u64")]
    pub effective_balance: u64,
    pub slashed: bool,

    /// When criteria for activation were met
    #[serde(with = "serde_utils::quoted_u64")]
    pub activation_eligibility_epoch: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub activation_epoch: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub exit_epoch: u64,

    /// When validator can withdraw funds
    #[serde(with = "serde_utils::quoted_u64")]
    pub withdrawable_epoch: u64,
}

impl Validator {
    /// Check if ``validator`` has an 0x01 prefixed "eth1" withdrawal credential.
    pub fn has_eth1_withdrawal_credential(&self) -> bool {
        &self.withdrawal_credentials[..1] == ETH1_ADDRESS_WITHDRAWAL_PREFIX
    }

    /// Check if ``validator`` is fully withdrawable.
    pub fn is_fully_withdrawable_validator(&self, balance: u64, epoch: u64) -> bool {
        self.has_execution_withdrawal_credential()
            && self.withdrawable_epoch <= epoch
            && balance > 0
    }

    /// Check if ``validator`` is partially withdrawable.
    pub fn is_partially_withdrawable_validator(&self, balance: u64) -> bool {
        let max_effective_balance = self.get_max_effective_balance();
        self.has_execution_withdrawal_credential()
            && self.effective_balance == max_effective_balance
            && balance > max_effective_balance
    }

    pub fn is_slashable_validator(&self, epoch: u64) -> bool {
        !self.slashed && self.activation_epoch <= epoch && epoch < self.withdrawable_epoch
    }

    pub fn is_active_validator(&self, epoch: u64) -> bool {
        self.activation_epoch <= epoch && epoch < self.exit_epoch
    }

    /// Check if ``validator`` is eligible to be placed into the activation queue.
    pub fn is_eligible_for_activation_queue(&self) -> bool {
        self.activation_eligibility_epoch == FAR_FUTURE_EPOCH
            && self.effective_balance >= MIN_ACTIVATION_BALANCE
    }

    /// Check if ``validator`` has an 0x02 prefixed "compounding" withdrawal credential.
    pub fn has_compounding_withdrawal_credential(&self) -> bool {
        is_compounding_withdrawal_credential(self.withdrawal_credentials)
    }

    /// Check if ``validator`` has a 0x01 or 0x02 prefixed withdrawal credential.
    pub fn has_execution_withdrawal_credential(&self) -> bool {
        self.has_compounding_withdrawal_credential() || self.has_eth1_withdrawal_credential()
    }

    /// Get max effective balance for ``validator``.
    pub fn get_max_effective_balance(&self) -> u64 {
        if self.has_compounding_withdrawal_credential() {
            MAX_EFFECTIVE_BALANCE_ELECTRA
        } else {
            MIN_ACTIVATION_BALANCE
        }
    }
}
