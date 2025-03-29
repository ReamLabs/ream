use std::{net::SocketAddr, sync::Arc};

use config::ServerConfig;
use routes::get_routes;
use types::genesis::Genesis;
use utils::error::handle_rejection;
use warp::Filter;

pub mod config;
pub mod handlers;
pub mod routes;
pub mod types;
pub mod utils;

/// Start the Beacon API server.
pub async fn start_server(ctx: Arc<Genesis>, server_config: ServerConfig) {
    let addr: SocketAddr = format!("{}:{}", server_config.http_address, server_config.http_port)
        .parse()
        .expect("Unable to read ServerConfig");

    let routes = get_routes(ctx).recover(handle_rejection);

    println!("Starting server on {}", addr);
    warp::serve(routes).run(addr).await;
}
