use std::{net::SocketAddr, sync::Arc};

use config::ServerConfig;
use ream_network_spec::networks::NetworkSpec;
use routes::get_routes;
use tracing::info;
use utils::error::handle_rejection;
use warp::{filters::log::Info, Filter};

pub mod config;
pub mod handlers;
pub mod routes;
pub mod types;
pub mod utils;

/// Start the Beacon API server.
pub async fn start_server(network_spec: Arc<NetworkSpec>, server_config: ServerConfig) {
    let routes = get_routes(network_spec).recover(handle_rejection);

    info!("Starting server on {:?}", server_config.http_socket_address);
    warp::serve(routes).run(server_config.http_socket_address).await;
}
