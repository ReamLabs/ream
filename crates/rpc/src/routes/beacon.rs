use std::sync::Arc;

use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{
    Filter, Rejection, body,
    filters::{path::end, query::query},
    get, log, path, post,
    reply::Reply,
};

use super::with_db;
use crate::{
    handlers::{
        block::{get_block_attestations, get_block_from_id, get_block_rewards, get_block_root},
        checkpoint::get_finality_checkpoint,
        fork::get_fork,
        genesis::get_genesis,
        header::get_headers,
        randao::get_randao_mix,
        state::{get_pending_partial_withdrawals, get_state_root},
        validator::{
            get_validator_from_state, get_validators_from_state, post_validators_from_state,
        },
    },
    types::{
        errors::ApiError,
        id::{ID, ValidatorID},
        query::{IdQuery, ParentRootQuery, RandaoQuery, SlotQuery, StatusQuery},
        request::ValidatorsPostRequest,
    },
    utils::error::parsed_param,
};

const MAX_VALIDATOR_COUNT: usize = 100;

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

    let headers = beacon_base
        .and(path("headers"))
        .and(query::<SlotQuery>())
        .and(query::<ParentRootQuery>())
        .and(get())
        .and(db_filter.clone())
        .and_then({
            move |slot: SlotQuery, parent_root: ParentRootQuery, db: ReamDB| {
                get_headers(slot, parent_root, db)
            }
        })
        .with(log("headers"));

    let validators = beacon_base
        .and(path("states"))
        .and(parsed_param::<ID>())
        .and(path("validators"))
        .and(get())
        .and(query::<IdQuery>())
        .and(query::<StatusQuery>())
        .and(db_filter.clone())
        .and_then(
            move |state_id: ID, id_query: IdQuery, status_query: StatusQuery, db: ReamDB| async move {
                if let Some(validator_ids) = &id_query.id {
                    if validator_ids.len() >= MAX_VALIDATOR_COUNT {
                        return Err(warp::reject::custom(
                            ApiError::TooManyValidatorsIds(),
                        ));
                    }
                }
                get_validators_from_state(state_id, id_query, status_query, db).await
            },
        )
        .with(log("validators"));

    let post_validators = beacon_base
        .and(path("states"))
        .and(parsed_param::<ID>())
        .and(path("validators"))
        .and(end())
        .and(post())
        .and(body::json::<ValidatorsPostRequest>())
        .and(db_filter.clone())
        .and_then(
            move |state_id: ID, request: ValidatorsPostRequest, db: ReamDB| {
                post_validators_from_state(state_id, request, db)
            },
        )
        .with(log("post_validators"));

    let block_root = beacon_base
        .and(path("blocks"))
        .and(parsed_param::<ID>())
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
    let pending_partial_withdrawals = beacon_base
        .and(path("states"))
        .and(parsed_param::<ID>())
        .and(path("pending_partial_withdrawals"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_pending_partial_withdrawals(state_id, db))
        .with(log("pending_partial_withdrawals"));

    genesis
        .or(validator)
        .or(validators)
        .or(post_validators)
        .or(randao)
        .or(fork)
        .or(checkpoint)
        .or(state_root)
        .or(block_root)
        .or(block_rewards)
        .or(pending_partial_withdrawals)
        .or(headers)
}

pub fn get_beacon_routes_v2(
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let db_filter = with_db(db);
    let beacon_base = path("beacon");

    let block = beacon_base
        .and(path("blocks"))
        .and(parsed_param::<ID>())
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(get_block_from_id)
        .with(log("block"));

    let attestation = beacon_base
        .and(path("blocks"))
        .and(parsed_param::<ID>())
        .and(path("attestations"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |block_id: ID, db: ReamDB| get_block_attestations(block_id, db))
        .with(log("attestations"));

    block.or(attestation)
}
