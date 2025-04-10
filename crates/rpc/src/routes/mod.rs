use std::sync::Arc;

use beacon::{get_beacon_routes, get_beacon_routes_v2};
use config::get_config_routes;
use debug::get_debug_routes_v2;
use node::get_node_routes;
use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{Filter, Rejection, path, reply::Reply};

pub mod beacon;
pub mod config;
pub mod debug;
pub mod node;

fn get_v1_routes(
    network_spec: Arc<NetworkSpec>,
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let eth_base_v1 = path("eth").and(path("v1"));

    let beacon_routes = get_beacon_routes(network_spec.clone(), db.clone());

    let node_routes = get_node_routes();

    let config_routes = get_config_routes(network_spec.clone());

    eth_base_v1.and(beacon_routes.or(node_routes).or(config_routes))
}

fn get_v2_routes(db: ReamDB) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let eth_base_v2 = path("eth").and(path("v2"));

    let debug_routes_v2 = get_debug_routes_v2(db.clone());

    let beacon_routes_v2 = get_beacon_routes_v2(db.clone());

    eth_base_v2.and(debug_routes_v2.or(beacon_routes_v2))
}

/// Creates and returns all possible routes.
pub fn get_routes(
    network_spec: Arc<NetworkSpec>,
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let v1_routes = get_v1_routes(network_spec.clone(), db.clone());
    let v2_routes = get_v2_routes(db.clone());

    v2_routes.or(v1_routes)
}

/// Creates a filter for DB.
fn with_db(
    db: ReamDB,
) -> impl Filter<Extract = (ReamDB,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}
