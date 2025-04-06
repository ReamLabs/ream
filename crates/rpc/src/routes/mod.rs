use std::sync::Arc;

use beacon::get_beacon_routes;
use config::get_config_routes;
use debug::get_debug_routes;
use node::get_node_routes;
use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{Filter, Rejection, path, reply::Reply};

pub mod beacon;
pub mod config;
pub mod debug;
pub mod node;

/// Creates and returns all possible routes.
pub fn get_routes(
    network_spec: Arc<NetworkSpec>,
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let eth_base_v1 = path("eth").and(path("v1"));
    let eth_base_v2 = path("eth").and(path("v2"));

    let beacon_routes = get_beacon_routes(network_spec.clone(), db.clone());

    let node_routes = get_node_routes();

    let config_routes = get_config_routes(network_spec.clone());

    let debug_routes = get_debug_routes(db.clone());

    let combined_routes = beacon_routes
        .or(node_routes)
        .or(config_routes)
        .or(debug_routes);
    let versioned_base = eth_base_v1
        .or(eth_base_v2)
        .and_then(|_| async { Ok::<_, Rejection>(()) });
    versioned_base.and(combined_routes).map(|_, reply| reply)
}

/// Creates a filter for DB.
fn with_db(
    db: ReamDB,
) -> impl Filter<Extract = (ReamDB,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}
