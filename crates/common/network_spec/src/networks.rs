use std::sync::{Arc, LazyLock};

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
}

pub static MAINNET: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Mainnet,
    }
    .into()
});

pub static HOLESKY: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Holesky,
    }
    .into()
});

pub static SEPOLIA: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Sepolia,
    }
    .into()
});

pub static HOODI: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Hoodi,
    }
    .into()
});

pub static DEV: LazyLock<Arc<NetworkSpec>> = LazyLock::new(|| {
    NetworkSpec {
        network: Network::Dev,
    }
    .into()
});
