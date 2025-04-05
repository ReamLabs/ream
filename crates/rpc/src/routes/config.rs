use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use warp::{Filter, Rejection, filters::path::end, get, log, path, reply::Reply};

use crate::handlers::config::get_deposit_contract;

/// Creates and returns all `/config` routes.
pub fn get_config_routes(
    network_spec: Arc<NetworkSpec>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    path("config")
        .and(path("deposit_contract"))
        .and(end())
        .and(get())
        .and_then(move || get_deposit_contract(network_spec.chain_id, network_spec.deposit_contract_address))
        .with(log("deposit_contract"))
}
