use std::sync::{Arc, LazyLock};

use alloy_primitives::{b256, fixed_bytes};
use ream_consensus::genesis::Genesis;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Holesky,
    Sepolia,
    Hoodi,
    Dev,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkSpec {
    pub network: Network,
    pub genesis: Genesis,
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
    }
    .into()
});
