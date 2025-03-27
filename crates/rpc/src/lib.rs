use std::{net::SocketAddr, sync::Arc};

use config::ServerConfig;
use routes::get_routes;
use utils::{chain::BeaconChain, error::handle_rejection};
use warp::Filter;

pub mod config;
pub mod handlers;
pub mod routes;
pub mod types;
pub mod utils;

/// Start the Beacon API server.
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
    use serde_json::to_string_pretty;
    use warp::test::request;

    use super::*;

    #[tokio::test]
    async fn test_health_route() {
        let expected_result = to_string_pretty(&BeaconChain::mock_init()).unwrap();
        // Correct route definition
        let genesis_route = warp::path!("eth" / "v1" / "beacon" / "genesis")
            .map(move || warp::reply::json(&expected_result.clone()));

        // Correct request path
        let response = request()
            .method("GET")
            .path("/eth/v1/beacon/genesis")
            .reply(&genesis_route)
            .await;

        // Check response
        assert_eq!(response.status(), 200);
        let body: serde_json::Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body, to_string_pretty(&BeaconChain::mock_init()).unwrap());
    }
}
