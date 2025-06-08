use ream_consensus::validator::Validator;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidatorStatus {
    Pending,
    PendingInitialized,
    PendingQueued,
    Active,
    ActiveOngoing,
    ActiveExiting,
    ActiveSlashed,
    Exited,
    ExitedUnslashed,
    ExitedSlashed,
    Withdrawal,
    WithdrawalPossible,
    WithdrawalDone,
    Offline,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValidatorData {
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub balance: u64,
    pub status: ValidatorStatus,
    pub validator: Validator,
}

impl ValidatorData {
    pub fn new(index: u64, balance: u64, status: ValidatorStatus, validator: Validator) -> Self {
        Self {
            index,
            balance,
            status,
            validator,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ValidatorBalance {
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub balance: u64,
}
