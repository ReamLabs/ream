use alloy_primitives::{aliases::B32, b256, hex, B256};
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
            genesis_time: 1606824023,
            genesis_validator_root: b256!(
                "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"
            ),
            genesis_fork_version: B32::from_slice(&hex::decode("00000000").unwrap()),
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
