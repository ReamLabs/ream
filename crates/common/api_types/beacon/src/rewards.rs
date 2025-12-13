use serde::{Deserialize, Serialize};

/// Ideal rewards for a validator with perfect participation for a given effective balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdealReward {
    /// The validator's effective balance in Gwei
    #[serde(with = "serde_utils::quoted_u64")]
    pub effective_balance: u64,
    /// Reward for voting on the correct head
    #[serde(with = "serde_utils::quoted_u64")]
    pub head: u64,
    /// Reward for voting on the correct target
    #[serde(with = "serde_utils::quoted_u64")]
    pub target: u64,
    /// Reward for voting on the correct source
    #[serde(with = "serde_utils::quoted_u64")]
    pub source: u64,
    /// Inclusion delay reward (always 0 for post-Altair)
    #[serde(with = "serde_utils::quoted_u64")]
    pub inclusion_delay: u64,
    /// Inactivity penalty (0 for ideal case with perfect participation)
    #[serde(with = "serde_utils::quoted_u64")]
    pub inactivity: u64,
}

/// Actual rewards earned by a specific validator based on their participation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalReward {
    /// The validator's index
    #[serde(with = "serde_utils::quoted_u64")]
    pub validator_index: u64,
    /// Reward for voting on the correct head
    #[serde(with = "serde_utils::quoted_i64")]
    pub head: i64,
    /// Reward for voting on the correct target
    #[serde(with = "serde_utils::quoted_i64")]
    pub target: i64,
    /// Reward for voting on the correct source
    #[serde(with = "serde_utils::quoted_i64")]
    pub source: i64,
    /// Inclusion delay reward (always 0 for post-Altair)
    #[serde(with = "serde_utils::quoted_u64")]
    pub inclusion_delay: u64,
    /// Inactivity penalty (negative value, but serialized as positive per spec)
    #[serde(with = "serde_utils::quoted_i64")]
    pub inactivity: i64,
}

/// Data containing both ideal and total rewards for validators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationRewardsData {
    /// Ideal rewards grouped by unique effective balances
    pub ideal_rewards: Vec<IdealReward>,
    /// Total rewards for each validator
    pub total_rewards: Vec<TotalReward>,
}

/// Response for attestation rewards endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationRewardsResponse {
    /// Whether the response is based on optimistic execution
    pub execution_optimistic: bool,
    /// Whether the data is from a finalized epoch
    pub finalized: bool,
    /// The attestation rewards data
    pub data: AttestationRewardsData,
}
