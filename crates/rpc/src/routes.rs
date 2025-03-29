use std::sync::Arc;

use warp::{reply::Reply, Filter};

use crate::{handlers::genesis::get_genesis, types::genesis::Genesis};

/// Creates and returns all possible routes.
pub fn get_routes(
    ctx: Arc<Genesis>,
) -> impl Filter<Extract = impl Reply, Error = warp::Rejection> + Clone {
    let eth_base = warp::path("eth")
        .and(warp::path("v1"))
        .and(warp::path("beacon"));

    let genesis_clone: Arc<Genesis> = Arc::clone(&ctx);
    let genesis = eth_base
        .and(warp::path("genesis"))
        .and(warp::get())
        .and_then(move || get_genesis(genesis_clone.clone()))
        .with(warp::log("genesis"));

    #[allow(clippy::let_and_return)]
    let routes = genesis;
    routes
}
