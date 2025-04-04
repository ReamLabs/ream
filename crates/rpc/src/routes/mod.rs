use std::sync::Arc;

use beacon::get_beacon_routes;
use node::get_node_routes;
use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{Filter, Rejection, path, reply::Reply};

pub mod beacon;
pub mod node;

/// Creates and returns all possible routes.
pub fn get_routes(
    network_spec: Arc<NetworkSpec>,
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let eth_base = path("eth").and(path("v1"));

    let beacon_routes = get_beacon_routes(network_spec, db);

    let node_routes = get_node_routes();

    eth_base.and(beacon_routes.or(node_routes))
}
