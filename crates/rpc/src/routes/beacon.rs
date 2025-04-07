use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{
    Filter, Rejection,
    filters::{
        path::{end, param},
        query::query,
    },
    get, log, path,
    reply::Reply,
};

use super::with_db;
use crate::{
    handlers::{
        checkpoint::get_finality_checkpoint, fork::get_fork, genesis::get_genesis,
        randao::get_randao_mix, state::get_state_root, validator::get_validator_from_state,
    },
    types::{
        id::{ID, ValidatorID},
        query::RandaoQuery,
    },
};

/// Creates and returns all `/beacon` routes.
pub fn get_beacon_routes(
    network_spec: Arc<NetworkSpec>,
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let beacon_base = path("beacon");
    let db_filter = with_db(db);

    let genesis = beacon_base
        .and(path("genesis"))
        .and(end())
        .and(get())
        .and_then(move || get_genesis(network_spec.genesis.clone()))
        .with(log("genesis"));

    let fork = beacon_base
        .and(path("states"))
        .and(param::<ID>())
        .and(path("fork"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_fork(state_id, db))
        .with(log("fork"));

    let state_root = beacon_base
        .and(path("states"))
        .and(param::<ID>())
        .and(path("root"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_state_root(state_id, db))
        .with(log("state_root"));

    let randao = beacon_base
        .and(path("states"))
        .and(param::<ID>())
        .and(path("randao"))
        .and(query::<RandaoQuery>())
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, query: RandaoQuery, db: ReamDB| {
            get_randao_mix(state_id, query, db)
        })
        .with(log("randao"));

    let checkpoint = beacon_base
        .and(path("states"))
        .and(param::<ID>())
        .and(path("finality_checkpoints"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_finality_checkpoint(state_id, db));

    let validator = beacon_base
        .and(path("states"))
        .and(param::<ID>())
        .and(path("validator"))
        .and(param::<ValidatorID>())
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then({
            move |state_id: ID, validator_id: ValidatorID, db: ReamDB| {
                get_validator_from_state(state_id, validator_id, db)
            }
        })
        .with(log("validator"));

    genesis
        .or(validator)
        .or(randao)
        .or(fork)
        .or(checkpoint)
        .or(state_root)
}
