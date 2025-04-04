use std::{str::FromStr, sync::Arc};

use ream_network_spec::networks::NetworkSpec;
use ream_storage::db::ReamDB;
use warp::{
    Filter, Rejection,
    filters::path::{end, param},
    get, log, path,
    reject::custom,
    reply::Reply,
};

use crate::{
    handlers::{genesis::get_genesis, validator::get_validator_from_state},
    types::{
        errors::ApiError,
        id::{ID, ValidatorID},
    },
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
            .and(param::<String>())
            .and(path("validator"))
            .and(param::<String>())
            .and_then(
                |state_id_str: String, validator_id_str: String| async move {
                    let state_id = ID::from_str(&state_id_str)
                        .map_err(|_| custom(ApiError::InvalidParameter(state_id_str.clone())))?;

                    let validator_id = ValidatorID::from_str(&validator_id_str).map_err(|_| {
                        custom(ApiError::InvalidParameter(validator_id_str.clone()))
                    })?;

                    Ok::<_, Rejection>((state_id, validator_id))
                },
            )
            .and(end())
            .and(get())
            .and_then({
                move |(state_id, validator_id): (ID, ValidatorID)| {
                    get_validator_from_state(state_id, validator_id, db.clone())
                }
            })
    };

    genesis.or(validator)
}
