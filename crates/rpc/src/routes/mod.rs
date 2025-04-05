use std::sync::Arc;

use beacon::get_beacon_routes;
use config::get_config_routes;
use node::get_node_routes;
use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{Filter, Rejection, path, reply::Reply};

pub mod beacon;
pub mod config;
pub mod node;

/// Creates and returns all possible routes.
pub fn get_routes(
    network_spec: Arc<NetworkSpec>,
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let eth_base = path("eth").and(path("v1"));

    let beacon_routes = get_beacon_routes(network_spec.clone(), db);

    let node_routes = get_node_routes();

    let config_routes = get_config_routes(network_spec.clone());

    eth_base.and(beacon_routes.or(node_routes).or(config_routes))
}

/// Creates a filter for DB.
fn with_db(
    db: ReamDB,
) -> impl Filter<Extract = (ReamDB,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}
