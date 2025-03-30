use std::sync::Arc;

use ream_network_spec::networks::{Network, NetworkSpec};
use warp::{reply::Reply, Filter};

use crate::handlers::genesis::get_genesis;

/// Creates and returns all possible routes.
pub fn get_routes(
    ctx: Arc<NetworkSpec>,
) -> impl Filter<Extract = impl Reply, Error = warp::Rejection> + Clone {
    let eth_base = warp::path("eth")
        .and(warp::path("v1"))
        .and(warp::path("beacon"));

    let genesis = eth_base
        .and(warp::path("genesis"))
        .and(warp::get())
        .and_then(move || get_genesis(ctx.genesis.clone()))
        .with(warp::log("genesis"));

    genesis
}
