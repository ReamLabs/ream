use alloy_primitives::{aliases::B32, B256};
use serde::{Deserialize, Serialize};

/// Genesis Config store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genesis {
    pub genesis_time: usize,
    pub genesis_validator_root: B256,
    pub genesis_fork_version: B32,
}

impl PartialEq for Genesis {
    fn eq(&self, other: &Self) -> bool {
        self.genesis_time == other.genesis_time
            && self.genesis_validator_root == other.genesis_validator_root
            && self.genesis_fork_version == other.genesis_fork_version
    }
}

impl Eq for Genesis {}
