use alloy_primitives::B256;
use ream_api_types_common::{error::ApiError, id::ID};

pub mod availability;
pub mod column;
pub mod health;
pub mod ingest;
pub mod retention;

/// Resolve a request-path [`ID`] to a concrete block root.
///
/// The DA node stores by root and keeps no chain of its own, so it can only
/// accept [`ID::Root`].
pub(crate) fn block_root_from_id(id: ID) -> Result<B256, ApiError> {
    match id {
        ID::Root(root) => Ok(root),
        other => Err(ApiError::BadRequest(format!(
            "the DA node identifies blocks by root only; `{other}` is not supported"
        ))),
    }
}

/// Resolve a request-path [`ID`] to a concrete slot.
///
/// Retention boundaries are expressed as a slot, so this accpet only
/// [`ID::Slot`].
pub(crate) fn slot_from_id(id: ID) -> Result<u64, ApiError> {
    match id {
        ID::Slot(slot) => Ok(slot),
        other => Err(ApiError::BadRequest(format!(
            "the DA node accepts a concrete slot only; `{other}` is not supported"
        ))),
    }
}
