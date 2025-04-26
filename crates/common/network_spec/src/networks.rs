use std::sync::{Arc, LazyLock};

use alloy_primitives::{Address, address, b256, fixed_bytes};
use ream_consensus::genesis::Genesis;

use crate::fork_schedule::{
    DEV_FORK_SCHEDULE, ForkSchedule, HOLESKY_FORK_SCHEDULE, HOODI_FORK_SCHEDULE,
    MAINNET_FORK_SCHEDULE, SEPOLIA_FORK_SCHEDULE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Holesky,
    Sepolia,
    Hoodi,
    Dev,
}

impl Network {
    pub fn chain_id(&self) -> u64 {
        match self {
            Network::Mainnet => 1,
            Network::Holesky => 17000,
            Network::Sepolia => 11155111,
            Network::Hoodi => 560048,
            Network::Dev => 1,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkSpec {
    pub network: Network,
    pub genesis: Genesis,
    pub deposit_contract_address: Address,
    pub fork_schedule: ForkSchedule,
}

pub static MAINNET: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Mainnet,
        genesis: Genesis {
            genesis_time: 1606824023,
            genesis_validator_root: b256!(
                "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"
            ),
            genesis_fork_version: fixed_bytes!("0x00000000"),
        },
        deposit_contract_address: address!("0x00000000219ab540356cBB839Cbe05303d7705Fa"),
        fork_schedule: MAINNET_FORK_SCHEDULE,
    }
    .into()
});

pub static HOLESKY: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Holesky,
        genesis: Genesis {
            genesis_time: 1727505000,
            genesis_validator_root: b256!(
                "0x9143aa7c615a7f7115e2b6aac319c03529df8242ae705fba9df39b79c59fa8b1"
            ),
            genesis_fork_version: fixed_bytes!("0x01017000"),
        },
        deposit_contract_address: address!("0x4242424242424242424242424242424242424242"),
        fork_schedule: HOLESKY_FORK_SCHEDULE,
    }
    .into()
});

pub static SEPOLIA: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Sepolia,
        genesis: Genesis {
            genesis_time: 1655713800,
            genesis_validator_root: b256!(
                "0xd8ea171f3c94aea21ebc42a1ed61052acf3f9209c00e4efbaaddac09ed9b8078"
            ),
            genesis_fork_version: fixed_bytes!("0x90000069"),
        },
        deposit_contract_address: address!("0x7f02C3E3c98b133055B8B348B2Ac625669Ed295D"),
        fork_schedule: SEPOLIA_FORK_SCHEDULE,
    }
    .into()
});

pub static HOODI: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Hoodi,
        genesis: Genesis {
            genesis_time: 1742193600,
            genesis_validator_root: b256!(
                "0x212f13fc4df078b6cb7db228f1c8307566dcecf900867401a92023d7ba99cb5f"
            ),
            genesis_fork_version: fixed_bytes!("0x10000910"),
        },
        deposit_contract_address: address!("0x00000000219ab540356cBB839Cbe05303d7705Fa"),
        fork_schedule: HOODI_FORK_SCHEDULE,
    }
    .into()
});

pub static DEV: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Dev,
        genesis: Genesis {
            genesis_time: 1606824023,
            genesis_validator_root: b256!(
                "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"
            ),
            genesis_fork_version: fixed_bytes!("0x00000000"),
        },
        deposit_contract_address: address!("0x00000000219ab540356cBB839Cbe05303d7705Fa"),
        fork_schedule: DEV_FORK_SCHEDULE,
    }
    .into()
});
