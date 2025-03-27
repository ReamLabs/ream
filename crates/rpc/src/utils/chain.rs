use alloy_primitives::{aliases::B32, b256, B256};
use serde::{Deserialize, Serialize};

/// Config for Beacon Chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconChain {
    pub genesis_time: u64,
    pub genesis_validator_root: B256,
    pub genesis_fork_version: B32,
}

impl BeaconChain {
    /// Mock the `/genesis` call for testing purposes.
    pub fn mock_init() -> Self {
        Self {
            genesis_time: 15908232934,
            genesis_validator_root: b256!(
                "0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2"
            ),
            genesis_fork_version: b"0x00".into(),
        }
    }

    pub fn new(genesis_time: u64, genesis_validator_root: B256, genesis_fork_version: B32) -> Self {
        Self {
            genesis_time,
            genesis_validator_root,
            genesis_fork_version,
        }
    }
}
