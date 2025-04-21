use alloy_primitives::{B256, aliases::B32};
use serde::{Deserialize, Serialize};

/// Genesis Config store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Genesis {
    #[serde(with = "serde_utils::quoted_u64")]
    pub genesis_time: u64,
    pub genesis_validators_root: B256,
    pub genesis_fork_version: B32,
}
