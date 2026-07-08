use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_data_availability_node::ingest::{IngestHandle, RetentionHint};

use crate::handlers::{slot_from_id, submission_error};

/// `POST /data/v0/retention/{slot}` — prune every stored column whose slot is
/// strictly below `{slot}`. The hint rides the verification queue, so pruning
/// stays serialized with verification and off the request path.
#[post("/retention/{slot}")]
pub async fn post_retention(
    handle: Data<IngestHandle>,
    slot: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let slot = slot_from_id(slot.into_inner())?;
    handle
        .try_submit_retention(RetentionHint { slot })
        .map_err(submission_error)?;

    Ok(HttpResponse::Accepted().finish())
}
