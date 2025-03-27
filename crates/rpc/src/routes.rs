use std::sync::Arc;

use utils::chain::BeaconChain;
use warp::{reply::Reply, Filter};

use crate::{handlers::genesis::get_genesis, utils};

/// Creates and returns all possible routes.
pub fn get_routes(
    ctx: Arc<BeaconChain>,
) -> impl Filter<Extract = impl Reply, Error = warp::Rejection> + Clone {
    let eth_base = warp::path("eth")
        .and(warp::path("v1"))
        .and(warp::path("beacon"));

    let beacon_clone: Arc<BeaconChain> = Arc::clone(&ctx);
    let genesis = eth_base
        .and(warp::path("genesis"))
        .and(warp::get())
        .and_then(move || get_genesis(beacon_clone.clone()))
        .with(warp::log("genesis"));

    #[allow(clippy::let_and_return)]
    let routes = genesis;
    routes
}
