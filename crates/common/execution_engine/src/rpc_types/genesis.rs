use alloy_primitives::{aliases::B32, B256};
use serde::{Deserialize, Serialize};

/// Genesis Config store.
#[derive(Debug, Clone, Serialize, Deserialize,PartialEq, Eq)]
pub struct Genesis {
    pub genesis_time: usize,
    pub genesis_validator_root: B256,
    pub genesis_fork_version: B32,
}

