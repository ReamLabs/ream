use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_da::{availability::ColumnAvailability, store::ColumnReadStore};
use serde::Serialize;

use crate::handlers::block_root_from_id;

/// JSON body of `GET /data/v0/availability/{block_root}`.
#[derive(Serialize)]
pub struct AvailabilityResponse {
    complete: bool,
    held_count: u64,
    missing: Vec<u64>,
}

impl From<ColumnAvailability> for AvailabilityResponse {
    fn from(availability: ColumnAvailability) -> Self {
        Self {
            complete: availability.is_complete(),
            held_count: availability.held_count(),
            missing: availability.missing_indices(),
        }
    }
}

/// `GET /data/v0/availability/{block_root}` — report which columns this node holds
/// for a block, and which it still expects.
#[get("/availability/{block_root}")]
pub async fn get_availability(
    store: Data<Arc<dyn ColumnReadStore>>,
    block_root: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let block_root = block_root_from_id(block_root.into_inner())?;
    let availability = store
        .availability(block_root)
        .map_err(|err| ApiError::InternalError(format!("availability lookup failed: {err}")))?;
    Ok(HttpResponse::Ok().json(AvailabilityResponse::from(availability)))
}
