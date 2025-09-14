pub mod config;
pub mod handlers;
pub mod routes;

use std::{collections::HashMap, sync::Arc};

use actix_web::web::Data;
use config::LeanRpcServerConfig;
use libp2p::PeerId;
use parking_lot::Mutex;
use ream_chain_lean::lean_chain::LeanChainReader;
use ream_p2p::network::peer::ConnectionState;
use ream_rpc_common::server::start_rpc_server;

use crate::routes::register_routers;

/// Start the Lean API server.
pub async fn start_lean_server(
    server_config: LeanRpcServerConfig,
    lean_chain: LeanChainReader,
    peer_table: Arc<Mutex<HashMap<PeerId, ConnectionState>>>,
) -> std::io::Result<()> {
    let server = start_rpc_server(server_config.http_socket_address, move |cfg| {
        cfg.app_data(Data::new(lean_chain.clone()))
            .app_data(Data::new(peer_table.clone()))
            .configure(register_routers);
    })?;

    server.await
}
