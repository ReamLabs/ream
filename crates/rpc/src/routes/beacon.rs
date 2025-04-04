use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{
    Filter, Rejection,
    filters::path::{end, param},
    get, log, path,
    reply::Reply,
};

use crate::handlers::{genesis::get_genesis, validator::get_validator_from_state};

/// Creates and returns all `/beacon` routes.
pub fn get_beacon_routes(
    network_spec: Arc<NetworkSpec>,
    db: Arc<ReamDB>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let beacon_base = path("beacon");

    let genesis = beacon_base
        .and(path("genesis"))
        .and(end())
        .and(get())
        .and_then(move || get_genesis(network_spec.genesis.clone()))
        .with(log("genesis"));

    let validator = beacon_base
        .and(path("states"))
        .and(param::<String>())
        .and(path("validator"))
        .and(param::<String>())
        .and(end())
        .and(get())
        .and_then({
            move |state_id: String, validator_id: String| {
                get_validator_from_state(state_id, validator_id, Arc::clone(&db))
            }
        });

    genesis.or(validator)
}
