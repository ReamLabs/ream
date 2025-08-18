pub mod config;
pub mod handlers;
pub mod routes;

use std::{collections::HashMap, sync::Arc};

use actix_web::{App, HttpServer, middleware, web::Data};
use config::LeanRpcServerConfig;
use libp2p::PeerId;
use parking_lot::Mutex;
use ream_chain_lean::lean_chain::LeanChainReader;
use ream_p2p::network::peer::ConnectionState;
use tracing::info;

use crate::routes::register_routers;

/// Start the Lean API server.
pub async fn start_lean_server(
    server_config: LeanRpcServerConfig,
    lean_chain: LeanChainReader,
    peer_table: Arc<Mutex<HashMap<PeerId, ConnectionState>>>,
) -> std::io::Result<()> {
    info!(
        "starting HTTP server on {:?}",
        server_config.http_socket_address
    );

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(Data::new(lean_chain.clone()))
            .app_data(Data::new(peer_table.clone()))
            .configure(register_routers)
    })
    .bind(server_config.http_socket_address)?
    .run();

    server.await
}
