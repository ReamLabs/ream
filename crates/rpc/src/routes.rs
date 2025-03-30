use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use warp::{Filter, Rejection, get, log, path, reply::Reply};

use crate::handlers::genesis::get_genesis;

/// Creates and returns all possible routes.
pub fn get_routes(
    network_spec: Arc<NetworkSpec>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let eth_base = path("eth").and(path("v1")).and(path("beacon"));

    eth_base
        .and(path("genesis"))
        .and(get())
        .and_then(move || get_genesis(network_spec.genesis.clone()))
        .with(log("genesis"))
}
