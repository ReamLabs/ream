use std::sync::Arc;

use config::ServerConfig;
use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use routes::get_routes;
use tracing::info;
use utils::error::handle_rejection;
use warp::{Filter, serve};

pub mod config;
pub mod handlers;
pub mod routes;
pub mod types;
pub mod utils;

/// Start the Beacon API server.
pub async fn start_server(network_spec: Arc<NetworkSpec>, server_config: ServerConfig, db: ReamDB) {
    let routes = get_routes(network_spec, db).recover(handle_rejection);

    info!("Starting server on {:?}", server_config.http_socket_address);
    serve(routes).run(server_config.http_socket_address).await;
}
