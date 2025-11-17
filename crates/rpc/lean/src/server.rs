use std::{io::Result, sync::Arc};

use ream_fork_choice_lean::store::LeanStoreReader;
use ream_network_state_lean::NetworkState;
use ream_rpc_common::{config::RpcServerConfig, server::RpcServerBuilder};

use crate::routes::register_routers;

/// Start the Lean API server.
pub async fn start(
    server_config: RpcServerConfig,
    lean_chain: LeanStoreReader,
    network_state: Arc<NetworkState>,
) -> Result<()> {
    RpcServerBuilder::new(server_config.http_socket_address)
        .allow_origin(server_config.http_allow_origin)
        .with_data(lean_chain)
        .with_data(network_state)
        .configure(register_routers)
        .start()
        .await
}
