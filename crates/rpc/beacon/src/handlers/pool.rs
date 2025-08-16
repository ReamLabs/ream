use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use ream_api_types_beacon::{
    error::ApiError,
    id::ID,
    responses::{DataResponse, DataVersionedResponse},
};
use ream_consensus_beacon::{
    attester_slashing::AttesterSlashing, bls_to_execution_change::SignedBLSToExecutionChange,
    voluntary_exit::SignedVoluntaryExit,
};
use ream_operation_pool::OperationPool;
use ream_storage::db::ReamDB;

use crate::handlers::state::get_state_from_id;

/// GET /eth/v1/beacon/pool/bls_to_execution_changes
#[get("/beacon/pool/bls_to_execution_changes")]
pub async fn get_bls_to_execution_changes(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    let signed_bls_to_execution_changes = operation_pool.get_signed_bls_to_execution_changes();
    Ok(HttpResponse::Ok().json(DataResponse::new(signed_bls_to_execution_changes)))
}

/// POST /eth/v1/beacon/pool/bls_to_execution_changes
#[post("/beacon/pool/bls_to_execution_changes")]
pub async fn post_bls_to_execution_changes(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
    signed_bls_to_execution_change: Json<SignedBLSToExecutionChange>,
) -> Result<impl Responder, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let signed_bls_to_execution_change = signed_bls_to_execution_change.into_inner();

    beacon_state
    .validate_bls_to_execution_change(&signed_bls_to_execution_change)
    .map_err(|err| {
        ApiError::BadRequest(format!(
            "Invalid bls_to_execution_change, it will never pass validation so it's rejected: {err:?}"
        ))
    })?;

    operation_pool.insert_signed_bls_to_execution_change(signed_bls_to_execution_change);
    // TODO: publish bls_to_execution_change to peers (gossipsub) - https://github.com/ReamLabs/ream/issues/556

    Ok(HttpResponse::Ok())
}

/// GET /eth/v1/beacon/pool/voluntary_exits
#[get("/beacon/pool/voluntary_exits")]
pub async fn get_voluntary_exits(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    let signed_voluntary_exits = operation_pool.get_signed_voluntary_exits();
    Ok(HttpResponse::Ok().json(DataResponse::new(signed_voluntary_exits)))
}

/// POST /eth/v1/beacon/pool/voluntary_exits
#[post("/beacon/pool/voluntary_exits")]
pub async fn post_voluntary_exits(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
    signed_voluntary_exit: Json<SignedVoluntaryExit>,
) -> Result<impl Responder, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let signed_voluntary_exit = signed_voluntary_exit.into_inner();

    beacon_state
        .validate_voluntary_exit(&signed_voluntary_exit)
        .map_err(|err| {
            ApiError::BadRequest(format!(
                "Invalid voluntary exit, it will never pass validation so it's rejected: {err:?}"
            ))
        })?;

    operation_pool.insert_signed_voluntary_exit(signed_voluntary_exit);
    // TODO: publish voluntary exit to peers (gossipsub) - https://github.com/ReamLabs/ream/issues/556

    Ok(HttpResponse::Ok())
}

/// GET /eth/v2/beacon/pool/attester_slashings
#[get("/beacon/pool/attester_slashings")]
pub async fn get_pool_attester_slashings(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    let attester_slashings = operation_pool.get_all_attester_slashings();
    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(attester_slashings)))
}

/// POST /eth/v2/beacon/pool/attester_slashings
#[post("/beacon/pool/attester_slashings")]
pub async fn post_pool_attester_slashings(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
    attester_slashing: Json<AttesterSlashing>,
) -> Result<impl Responder, ApiError> {
    let attester_slashing = attester_slashing.into_inner();

    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let _ = beacon_state
        .get_slashable_attester_indices(&attester_slashing)
        .map_err(|_| {
            ApiError::BadRequest(
                "Invalid attester slashing, it will never pass validation so it's rejected"
                    .to_string(),
            )
        })?;

    operation_pool.insert_attester_slashing(attester_slashing);
    // TODO: publish attester slashing to peers (gossipsub) - https://github.com/ReamLabs/ream/issues/556

    Ok(HttpResponse::Ok())
}
