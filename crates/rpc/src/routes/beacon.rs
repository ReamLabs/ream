use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{
    Filter, Rejection,
    filters::path::{end, param},
    get, log, path,
    reply::Reply,
};

use crate::{
    handlers::{genesis::get_genesis, validator::get_validator_from_state},
    types::id::{ID, ValidatorID},
};

/// Creates and returns all `/beacon` routes.
pub fn get_beacon_routes(
    network_spec: Arc<NetworkSpec>,
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let beacon_base = path("beacon");

    let genesis = beacon_base
        .and(path("genesis"))
        .and(end())
        .and(get())
        .and_then(move || get_genesis(network_spec.genesis.clone()))
        .with(log("genesis"));

    let validator = {
        beacon_base
            .and(path("states"))
            .and(param::<ID>())
            .and(path("validator"))
            .and(param::<ValidatorID>())
            .and(end())
            .and(get())
            .and_then({
                move |state_id: ID, validator_id: ValidatorID| {
                    get_validator_from_state(state_id, validator_id, db.clone())
                }
            })
    };

    genesis.or(validator)
}
