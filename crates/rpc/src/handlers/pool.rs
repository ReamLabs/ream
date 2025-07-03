use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use ream_beacon_api_types::{error::ApiError, id::ID, responses::DataResponse};
use ream_consensus::{
    bls_to_execution_change::SignedBLSToExecutionChange, voluntary_exit::SignedVoluntaryExit,
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
