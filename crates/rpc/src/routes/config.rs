use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use warp::{Filter, Rejection, filters::path::end, get, log, path, reply::Reply};

use crate::handlers::config::{get_deposit_contract, get_spec};

/// Creates and returns all `/config` routes.
/// Creates and returns all `/config` routes.
pub fn get_config_routes(
    network_spec: Arc<NetworkSpec>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    // Create a reusable filter that clones the Arc once
    let with_network_spec = warp::any().map(move || network_spec.clone());

    let deposit_contract = path("config")
        .and(path("deposit_contract"))
        .and(end())
        .and(get())
        .and(with_network_spec.clone())
        .and_then(|spec: Arc<NetworkSpec>| get_deposit_contract(spec))
        .with(log("deposit_contract"));

    let spec_config = path("config")
        .and(path("spec"))
        .and(end())
        .and(get())
        .and(with_network_spec)
        .and_then(|spec: Arc<NetworkSpec>| get_spec(spec))
        .with(log("spec_config"));

    deposit_contract.or(spec_config)
}
