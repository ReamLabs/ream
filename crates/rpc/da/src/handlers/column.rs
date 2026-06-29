use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use alloy_primitives::B256;
use ream_api_types_common::{error::ApiError, id::ID};
use ream_da::{column::VerifiedColumn, id::DaColumnId, store::DaReadStore};
use serde::Serialize;

use crate::handlers::block_root_from_id;

/// JSON view of a stored column.
///
/// The payload travels as a `0x`-hex string, symmetric with the
/// ingest envelope, instead of `DaPayload`'s raw byte array.
///
/// TODO: this hex-JSON form is the dev/debug interface, not the final wire
/// format. The serving path's real consumer is the local beacon, which wants the
/// bytes verbatim.
/// Keep a hex branch (e.g. `?encoding=hex`) purely for `curl`/debug.
#[derive(Serialize)]
pub struct ColumnResponse {
    block_root: B256,
    index: u64,
    slot: u64,
    /// `0x`-prefixed hex of the opaque column payload.
    payload: String,
}

impl From<VerifiedColumn> for ColumnResponse {
    fn from(column: VerifiedColumn) -> Self {
        let id = column.id();
        Self {
            block_root: id.block_root(),
            index: id.index(),
            slot: column.context().slot,
            payload: alloy_primitives::hex::encode_prefixed(column.payload().as_bytes()),
        }
    }
}

/// `GET /da/v0/columns/{block_root}/{index}` — serve a single stored column.
///
/// An out-of-range `index` is a client error (400). A column the node simply does
/// not hold is a 404.
#[get("/columns/{block_root}/{index}")]
pub async fn get_column(
    store: Data<Arc<dyn DaReadStore>>,
    path: Path<(ID, u64)>,
) -> Result<impl Responder, ApiError> {
    let (block_root, index) = path.into_inner();
    let block_root = block_root_from_id(block_root)?;
    let id = DaColumnId::new(block_root, index)
        .map_err(|err| ApiError::BadRequest(format!("invalid column id: {err}")))?;

    let column = store
        .get(&id)
        .map_err(|err| ApiError::InternalError(format!("column lookup failed: {err}")))?
        .ok_or_else(|| ApiError::NotFound(format!("column {index} for {block_root:?} not held")))?;

    Ok(HttpResponse::Ok().json(ColumnResponse::from(column)))
}

/// `GET /da/v0/columns/{block_root}` — serve every column this node holds for a
/// block (the sidecar data it can offer).
///
/// Walks the held set from the availability snapshot and reads each column.
/// Returns whatever is present, so an unknown block yields an empty array rather
/// than a 404 — the truthful "I hold nothing for this block".
#[get("/columns/{block_root}")]
pub async fn get_columns(
    store: Data<Arc<dyn DaReadStore>>,
    block_root: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let block_root = block_root_from_id(block_root.into_inner())?;
    let availability = store
        .availability(block_root)
        .map_err(|err| ApiError::InternalError(format!("availability lookup failed: {err}")))?;

    let mut columns = Vec::with_capacity(availability.held_count() as usize);
    for index in availability.held_indices() {
        let id = DaColumnId::new(block_root, index)
            .expect("held index comes from the store and is always in range");
        // A column can be pruned between the snapshot and this read; skip a
        // vanished one rather than failing the whole sidecar.
        if let Some(column) = store
            .get(&id)
            .map_err(|err| ApiError::InternalError(format!("column lookup failed: {err}")))?
        {
            columns.push(ColumnResponse::from(column));
        }
    }

    Ok(HttpResponse::Ok().json(columns))
}
