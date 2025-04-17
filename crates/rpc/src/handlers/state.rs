use actix_web::{HttpResponse, Responder, error, get, web};
use alloy_primitives::B256;
use ream_consensus::{
    checkpoint::Checkpoint, deneb::beacon_state::BeaconState, validator::Validator,
};
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use tree_hash::TreeHash;

use crate::types::{
    errors::ApiError,
    id::ID,
    query::RandaoQuery,
    response::{BeaconResponse, RootResponse},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValidatorData {
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    balance: u64,
    status: String,
    validator: Validator,
}

impl ValidatorData {
    pub fn new(index: u64, balance: u64, status: String, validator: Validator) -> Self {
        Self {
            index,
            balance,
            status,
            validator,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckpointData {
    previous_justified: Checkpoint,
    current_justified: Checkpoint,
    finalized: Checkpoint,
}

impl CheckpointData {
    pub fn new(
        previous_justified: Checkpoint,
        current_justified: Checkpoint,
        finalized: Checkpoint,
    ) -> Self {
        Self {
            previous_justified,
            current_justified,
            finalized,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RandaoResponse {
    pub randao: B256,
}

pub async fn get_state_from_id(state_id: ID, db: &ReamDB) -> Result<BeaconState, ApiError> {
    let block_root = match state_id {
        ID::Finalized => {
            let finalized_checkpoint = db
                .finalized_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Finalized checkpoint not found"))
                })?;

            Ok(Some(finalized_checkpoint.root))
        }
        ID::Justified => {
            let justified_checkpoint = db
                .justified_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Justified checkpoint not found"))
                })?;

            Ok(Some(justified_checkpoint.root))
        }
        ID::Head | ID::Genesis => {
            return Err(ApiError::NotFound(format!(
                "This ID type is currently not supported: {state_id:?}"
            )));
        }
        ID::Slot(slot) => db.slot_index_provider().get(slot),
        ID::Root(root) => db.state_root_index_provider().get(root),
    }
    .map_err(|_| ApiError::InternalError)?
    .ok_or(ApiError::NotFound(format!(
        "Failed to find `block_root` from {state_id:?}"
    )))?;

    db.beacon_state_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon_state` from {block_root:?}"
        )))
}

#[get("/beacon/states/{state_id}")]
pub async fn get_beacon_state(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
) -> actix_web::Result<impl Responder> {
    let state_id = state_id.into_inner();
    let state = get_state_from_id(state_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(state))
}

#[get("/beacon/states/{state_id}/root")]
pub async fn get_state_root(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
) -> actix_web::Result<impl Responder> {
    warn!("start get_state_root");
    let state_id = state_id.into_inner();
    let state = get_state_from_id(state_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let state_root = state.tree_hash_root();
    warn!("state_root: {:?}", state_root);

    Ok(HttpResponse::Ok().json(state_root.to_string()))
}

/// Called by `/eth/v1/beacon/states/{state_id}/fork` to get fork of state.
#[get("/beacon/states/{state_id}/fork")]
pub async fn get_state_fork(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
) -> actix_web::Result<impl Responder> {
    let state_id = state_id.into_inner();
    let state = get_state_from_id(state_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(state.fork))
}

/// Called by `/states/<state_id>/finality_checkpoints` to get the Checkpoint Data of state.
#[get("/beacon/states/{state_id}/finality_checkpoints")]
pub async fn get_state_finality_checkpoint(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
) -> actix_web::Result<impl Responder> {
    let state_id = state_id.into_inner();
    let state = get_state_from_id(state_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let response = CheckpointData::new(
        state.previous_justified_checkpoint,
        state.current_justified_checkpoint,
        state.finalized_checkpoint,
    );
    Ok(HttpResponse::Ok().json(response))
}

/// Called by `/states/<state_id>/randao` to get the Randao mix of state.
/// Pass optional `epoch` in the query to get randao for particular epoch,
/// else will fetch randao of the state epoch
#[get("/beacon/states/{state_id}/randao")]
pub async fn get_state_randao(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
    query: web::Json<RandaoQuery>,
) -> actix_web::Result<impl Responder> {
    let state_id = state_id.into_inner();
    let state = get_state_from_id(state_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let randao_mix = match query.epoch {
        Some(epoch) => state.get_randao_mix(epoch),
        None => state.get_randao_mix(state.get_current_epoch()),
    };

    let response = RandaoResponse { randao: randao_mix };
    Ok(HttpResponse::Ok().json(response))
}

#[get("/beacon/states/{state_id}/validator")]
pub async fn get_state_validator(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
    validator: web::Json<Validator>,
) -> actix_web::Result<impl Responder> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(error::ErrorInternalServerError)?
        .ok_or(error::ErrorNotFound("Failed to find highest slot"))?;

    let state = get_state_from_id(ID::Slot(highest_slot), &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    if validator.exit_epoch < state.get_current_epoch() {
        Ok("offline".to_string())
    } else {
        Ok("active_ongoing".to_string())
    }
}
