use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use warp::{Filter, Rejection, filters::path::end, get, log, path, reply::Reply};

use crate::handlers::config::{get_deposit_contract, get_spec};

/// Creates and returns all `/config` routes.
/// Creates and returns all `/config` routes.
pub fn get_config_routes(
    network_spec: Arc<NetworkSpec>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let deposit_network_spec = network_spec.clone();
    let spec_network_spec = network_spec.clone();

    let deposit_contract = path("config")
        .and(path("deposit_contract"))
        .and(end())
        .and(get())
        .and_then(move || get_deposit_contract(deposit_network_spec.clone()))
        .with(log("deposit_contract"));

    let spec_config = path("config")
        .and(path("spec"))
        .and(end())
        .and(get())
        .and_then(move || get_spec(spec_network_spec.clone()))
        .with(log("spec_config"));
    deposit_contract.or(spec_config)
}
