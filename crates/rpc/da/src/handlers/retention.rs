use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_da_node::{
    error::IngestionError,
    ingest::{DaIngestHandle, RetentionHint},
};

use crate::handlers::slot_from_id;

/// `POST /da/v0/retention/{slot}` — submit a beacon-issued retention boundary,
/// pruning every stored column whose slot is strictly below `{slot}`.
///
/// Like `/ingest`, the hint is handed to the verification queue rather than
/// applied here: the single consumer then serializes pruning with verification,
/// so the store keeps its single-writer invariant and the blocking prune stays
/// off the request path. Non-blocking — a full queue sheds load.
#[post("/retention/{slot}")]
pub async fn post_retention(
    handle: Data<DaIngestHandle>,
    slot: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let slot = slot_from_id(slot.into_inner())?;
    handle
        .try_submit_retention(RetentionHint { slot })
        .map_err(|err| match err {
            // TODO: a retryable 503 would fit `Overloaded` better than 500 once
            // `ApiError` grows one (mirrors `/ingest`).
            IngestionError::Overloaded => {
                ApiError::InternalError("verification queue is full; retry shortly".to_string())
            }
            IngestionError::Closed => {
                ApiError::InternalError("verification service is unavailable".to_string())
            }
        })?;

    Ok(HttpResponse::Accepted().finish())
}
