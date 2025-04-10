use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{
    Filter, Rejection,
    filters::{path::end, query::query},
    get, log, path,
    reply::Reply,
};

use super::with_db;
use crate::{
    handlers::{
        block::{get_block_attestations, get_block_rewards, get_block_root},
        checkpoint::get_finality_checkpoint,
        fork::get_fork,
        genesis::get_genesis,
        randao::get_randao_mix,
        state::get_state_root,
        validator::get_validator_from_state,
    },
    types::{
        id::{ID, ValidatorID},
        query::RandaoQuery,
    },
    utils::error::parsed_param,
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
        .and(parsed_param::<ID>())
        .and(path("fork"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_fork(state_id, db))
        .with(log("fork"));

    let state_root = beacon_base
        .and(path("states"))
        .and(parsed_param::<ID>())
        .and(path("root"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_state_root(state_id, db))
        .with(log("state_root"));

    let randao = beacon_base
        .and(path("states"))
        .and(parsed_param::<ID>())
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
        .and(parsed_param::<ID>())
        .and(path("finality_checkpoints"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_finality_checkpoint(state_id, db));

    let validator = beacon_base
        .and(path("states"))
        .and(parsed_param::<ID>())
        .and(path("validator"))
        .and(parsed_param::<ValidatorID>())
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then({
            move |state_id: ID, validator_id: ValidatorID, db: ReamDB| {
                get_validator_from_state(state_id, validator_id, db)
            }
        })
        .with(log("validator"));

    let block_root = beacon_base
        .and(path("blocks"))
        .and(parsed_param::<ID>())
        .and(path("root"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |block_id: ID, db: ReamDB| get_block_root(block_id, db))
        .with(log("block_root"));
    let block_rewards = beacon_base
        .and(path("blocks"))
        .and(parsed_param::<ID>())
        .and(path("rewards"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |block_id: ID, db: ReamDB| get_block_rewards(block_id, db))
        .with(log("block_rewards"));

    genesis
        .or(validator)
        .or(randao)
        .or(fork)
        .or(checkpoint)
        .or(state_root)
        .or(block_root)
        .or(block_rewards)
}

pub fn get_beacon_routes_v2(
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let db_filter = with_db(db);
    let beacon_base = path("beacon");

    beacon_base
        .and(path("blocks"))
        .and(parsed_param::<ID>())
        .and(path("attestations"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |block_id: ID, db: ReamDB| get_block_attestations(block_id, db))
        .with(log("attestations"))
}
