use actix_web::{App, HttpServer, middleware, web::Data};
use config::LeanRpcServerConfig;
use ream_chain_lean::lean_chain::LeanChainReader;
use tracing::info;

use crate::routes::register_routers;

pub mod config;
pub mod handlers;
pub mod routes;

/// Start the Lean API server.
pub async fn start_lean_server(
    server_config: LeanRpcServerConfig,
    lean_chain: LeanChainReader,
) -> std::io::Result<()> {
    info!(
        "starting HTTP server on {:?}",
        server_config.http_socket_address
    );

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(Data::new(lean_chain.clone()))
            .configure(register_routers)
    })
    .bind(server_config.http_socket_address)?
    .run();

    server.await
}
