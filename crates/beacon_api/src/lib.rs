use config::ServerConfig;
use ream_config::chain::BeaconChain;
use routes::get_routes;
use std::sync::Arc;
use std::{env, net::SocketAddr};
use tokio::fs;
use utils::error::handle_rejection;
use warp::http::header::ACCESS_CONTROL_ALLOW_ORIGIN;
use warp::hyper::Server;
use warp::Filter;
use warp::TlsServer;

pub mod config;
pub mod handlers;
pub mod routes;
pub mod types;
pub mod utils;

pub async fn start_server(ctx: Arc<BeaconChain>, server_config: ServerConfig) {
    let addr: SocketAddr = format!("{}:{}", server_config.http_address, server_config.http_port)
        .parse()
        .unwrap();

    let routes = get_routes(ctx).recover(handle_rejection);

    println!("Starting server on {}", addr);
    warp::serve(routes).run(addr).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use warp::test::request;

    #[tokio::test]
    async fn test_health_route() {
        let health_route = warp::path!("health").map(|| warp::reply::json(&"OK"));

        let response = request()
            .method("GET")
            .path("/health")
            .reply(&health_route)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), "\"OK\"");
    }
}
