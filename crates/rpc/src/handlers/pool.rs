use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use ream_beacon_api_types::{error::ApiError, responses::DataResponse};
use ream_consensus::voluntary_exit::SignedVoluntaryExit;
use ream_operation_pool::OperationPool;
use ream_storage::db::ReamDB;
use tracing::error;

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
    let beacon_state = db
        .beacon_state_provider()
        .last()
        .map_err(|err| {
            error!("Failed to get latest beacon_state, error: {err:?}");
            ApiError::InternalError
        })?
        .ok_or_else(|| ApiError::NotFound(String::from("Failed to find latest beacon_state")))?;

    let signed_voluntary_exit = signed_voluntary_exit.into_inner();

    beacon_state
        .validate_voluntary_exit(&signed_voluntary_exit)
        .map_err(|err| {
            ApiError::BadRequest(format!(
                "Invalid voluntary exit, it will never pass validation so it's rejected: {err:?}"
            ))
        })?;

    operation_pool.insert_signed_voluntary_exit(signed_voluntary_exit);
    // TODO: broadcast voluntary exit to peers

    Ok(HttpResponse::Ok())
}
