//! Wiring for the optional embedded reth execution layer.
//!
//! Both variants expose a `start` taking a config type, an executor, and the chain service. With
//! the feature off it takes `Disabled`, does nothing, and returns a future that never resolves, so
//! the node's `select!` never picks that arm.

#[cfg(feature = "reth")]
mod enabled {
    use std::{
        net::{IpAddr, SocketAddr},
        path::PathBuf,
    };

    use alloy_primitives::B256;
    use ream_chain_lean::service::LeanChainService;
    use ream_executor::ReamExecutor;
    use ream_reth_engine::handle::{RethHandle, RethNodeConfig, RethP2pConfig};
    use tracing::info;

    /// The reth-relevant CLI arguments, moved out of `LeanNodeConfig` at the call site.
    pub struct Args {
        pub datadir: PathBuf,
        pub rpc_address: IpAddr,
        pub rpc_port: u16,
        pub p2p_address: IpAddr,
        pub p2p_port: Option<u16>,
        pub p2p_secret: Option<B256>,
        pub trusted_peers: Vec<String>,
    }

    /// Boots reth in-process, hands the chain service its handle, and returns the node's exit
    /// future. That future owns the node, so it must be kept alive for the EL to keep running.
    pub async fn start(
        args: Args,
        executor: &ReamExecutor,
        chain_service: &mut LeanChainService,
    ) -> impl Future<Output = anyhow::Result<()>> + use<> {
        let (handle, mut node) = RethHandle::start(RethNodeConfig {
            runtime: Some(executor.runtime().handle().clone()),
            datadir: args.datadir,
            http_rpc: Some(SocketAddr::new(args.rpc_address, args.rpc_port)),
            p2p: args
                .p2p_port
                .zip(args.p2p_secret)
                .map(|(port, secret_key)| RethP2pConfig {
                    address: args.p2p_address,
                    port,
                    secret_key,
                    trusted_peers: args.trusted_peers,
                }),
        })
        .await
        .expect("failed to boot embedded reth execution layer");

        info!(
            "Embedded reth is started with genesis hash: {:?}",
            node.node.chain_spec().genesis_hash()
        );

        chain_service.set_reth_handle(handle).await;

        async move {
            (&mut node.node_exit_future)
                .await
                .map_err(|err| anyhow::anyhow!("embedded reth exited: {err:?}"))
        }
    }
}

#[cfg(not(feature = "reth"))]
mod disabled {
    use ream_chain_lean::service::LeanChainService;
    use ream_executor::ReamExecutor;

    pub struct Disabled;

    /// Takes `&mut LeanChainService` like the enabled variant so the binding in `run_lean_node`
    /// needs `mut` in both builds.
    pub async fn start(
        _args: Disabled,
        _executor: &ReamExecutor,
        _chain_service: &mut LeanChainService,
    ) -> impl Future<Output = anyhow::Result<()>> + use<> {
        std::future::pending()
    }
}

#[cfg(not(feature = "reth"))]
pub use disabled::{Disabled, start};
#[cfg(feature = "reth")]
pub use enabled::{Args, start};
