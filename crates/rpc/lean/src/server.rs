use std::{collections::HashMap, io::Result, sync::Arc};

use libp2p::PeerId;
use parking_lot::Mutex;
use ream_chain_lean::lean_chain::LeanChainReader;
use ream_p2p::network::peer::ConnectionState;
use ream_rpc_common::{config::RpcServerConfig, server::RpcServerBuilder};

use crate::routes::register_routers;

/// Start the Lean API server.
pub async fn start(
    server_config: RpcServerConfig,
    lean_chain: LeanChainReader,
    peer_table: Arc<Mutex<HashMap<PeerId, ConnectionState>>>,
) -> Result<()> {
    RpcServerBuilder::new(server_config.http_socket_address)
        .allow_origin(server_config.http_allow_origin)
        .with_data(lean_chain)
        .with_data(peer_table)
        .configure(register_routers)
        .start()
        .await
}
