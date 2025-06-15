use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use ream_beacon_api_types::{error::ApiError, id::ID, responses::DataResponse};
use ream_consensus::{proposer_slashing::ProposerSlashing, voluntary_exit::SignedVoluntaryExit};
use ream_operation_pool::OperationPool;
use ream_storage::{db::ReamDB, tables::slashing_pool::SlashingPool};
use tracing::error;

use crate::handlers::state::get_state_from_id;

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
            error!("Failed to get_highest_slot, error: {err:?}");
            ApiError::InternalError
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

/// GET /eth/v1/beacon/pool/proposer_slashings
#[get("/beacon/pool/proposer_slashings")]
pub async fn get_pool_proposer_slashings(
    slashing_pool: Data<Arc<SlashingPool>>,
) -> Result<impl Responder, ApiError> {
    let slashings = slashing_pool.get_all_proposer_slashings();
    Ok(HttpResponse::Ok().json(DataResponse::new(slashings)))
}

/// POST /eth/v1/beacon/pool/proposer_slashings
#[post("/beacon/pool/post_proposer_slashings")]
pub async fn post_pool_proposer_slashings(
    db: Data<ReamDB>,
    slashing_pool: Data<SlashingPool>,
    slashing: Json<ProposerSlashing>,
) -> Result<impl Responder, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            error!("Failed to get_highest_slot, error: {err:?}");
            ApiError::InternalError
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let mut beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;
    let proposer_index = slashing.signed_header_1.message.proposer_index;

    if slashing_pool.has_slashing_for_proposer(proposer_index) {
        return Err(ApiError::BadRequest(
            "Proposer slashing already exists for this validator".to_string(),
        ));
    }
    let slashing = slashing.into_inner();

    beacon_state
        .process_proposer_slashing(&slashing)
        .map_err(|err| {
            ApiError::BadRequest(format!(
                "Invalid proposer slashing, it will never pass validation so it's rejected: {err:?}"
            ))
        })?;
    slashing_pool
        .get_ref()
        .insert_proposer_slashing(slashing.clone())
        .map_err(|_| ApiError::BadRequest("Proposer slashing already in pool".to_string()))?;

    Ok(HttpResponse::Ok().finish())
}
