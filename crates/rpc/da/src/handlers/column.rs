use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use alloy_primitives::B256;
use ream_api_types_common::{error::ApiError, id::ID};
use ream_da::{column::VerifiedColumn, id::ColumnId, store::ColumnReadStore};
use serde::Serialize;

use crate::handlers::block_root_from_id;

/// JSON view of a stored column; the payload travels as a `0x`-hex string.
///
/// TODO: hex-JSON is the dev/debug interface, not the final wire format — the
/// local beacon wants the bytes verbatim.
#[derive(Serialize)]
pub struct ColumnResponse {
    block_root: B256,
    index: u64,
    slot: u64,
    payload: String,
}

impl From<VerifiedColumn> for ColumnResponse {
    fn from(column: VerifiedColumn) -> Self {
        let id = column.id();
        Self {
            block_root: id.block_root(),
            index: id.index(),
            slot: column.context().slot,
            payload: alloy_primitives::hex::encode_prefixed(column.payload()),
        }
    }
}

/// `GET /data/v0/columns/{block_root}/{index}` — serve a single stored column;
/// 400 for an out-of-range index, 404 when not held.
#[get("/columns/{block_root}/{index}")]
pub async fn get_column(
    store: Data<Arc<dyn ColumnReadStore>>,
    path: Path<(ID, u64)>,
) -> Result<impl Responder, ApiError> {
    let (block_root, index) = path.into_inner();
    let block_root = block_root_from_id(block_root)?;
    let id = ColumnId::new(block_root, index)
        .map_err(|err| ApiError::BadRequest(format!("invalid column id: {err}")))?;

    let column = store
        .get(&id)
        .map_err(|err| ApiError::InternalError(format!("column lookup failed: {err}")))?
        .ok_or_else(|| ApiError::NotFound(format!("column {index} for {block_root:?} not held")))?;

    Ok(HttpResponse::Ok().json(ColumnResponse::from(column)))
}

/// `GET /data/v0/columns/{block_root}` — serve every column this node holds for
/// a block; an unknown block yields an empty array, not a 404.
#[get("/columns/{block_root}")]
pub async fn get_columns(
    store: Data<Arc<dyn ColumnReadStore>>,
    block_root: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let block_root = block_root_from_id(block_root.into_inner())?;
    let availability = store
        .availability(block_root)
        .map_err(|err| ApiError::InternalError(format!("availability lookup failed: {err}")))?;

    let mut columns = Vec::with_capacity(availability.held_count() as usize);
    for index in availability.held_indices() {
        let id = ColumnId::new(block_root, index)
            .expect("held index comes from the store and is always in range");
        // A column can be pruned between the snapshot and this read; skip it.
        if let Some(column) = store
            .get(&id)
            .map_err(|err| ApiError::InternalError(format!("column lookup failed: {err}")))?
        {
            columns.push(ColumnResponse::from(column));
        }
    }

    Ok(HttpResponse::Ok().json(columns))
}
