pub mod availability;
pub mod column;
pub mod health;
pub mod ingest;
pub mod retention;

use alloy_primitives::B256;
use ream_api_types_common::{error::ApiError, id::ID};
use ream_data_availability_node::error::IngestionError;

/// Resolve a request-path [`ID`] to a concrete block root.
pub(crate) fn block_root_from_id(id: ID) -> Result<B256, ApiError> {
    match id {
        ID::Root(root) => Ok(root),
        other => Err(ApiError::BadRequest(format!(
            "the data node identifies blocks by root only; `{other}` is not supported"
        ))),
    }
}

/// Resolve a request-path [`ID`] to a concrete slot.
pub(crate) fn slot_from_id(id: ID) -> Result<u64, ApiError> {
    match id {
        ID::Slot(slot) => Ok(slot),
        other => Err(ApiError::BadRequest(format!(
            "the data node accepts a concrete slot only; `{other}` is not supported"
        ))),
    }
}

/// Map a queue submission error onto the RPC's status codes.
pub(crate) fn submission_error(err: IngestionError) -> ApiError {
    match err {
        IngestionError::Overloaded => {
            ApiError::ServiceUnavailable("verification queue is full; retry shortly".to_string())
        }
        IngestionError::Closed => {
            ApiError::InternalError("verification service is unavailable".to_string())
        }
    }
}
