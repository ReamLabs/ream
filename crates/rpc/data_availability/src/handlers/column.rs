use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get,
    http::header::ContentType,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_data_availability::{id::ColumnId, store::ColumnReadStore};
use ssz::Encode;
use ssz_types::VariableList;

use crate::handlers::{
    block_root_from_id,
    ingest::{WireBlockBatch, WireIndexedPayload},
};

/// Wrap a stored payload as a batch entry.
///
/// The bound can only be exceeded by a payload that never passed ingest, so a
/// failure here means the store handed back something impossible.
fn wire_entry(index: u64, payload: &[u8]) -> Result<WireIndexedPayload, ApiError> {
    Ok(WireIndexedPayload {
        index,
        payload: VariableList::new(payload.to_vec()).map_err(|err| {
            ApiError::InternalError(format!(
                "stored column {index} exceeds the payload bound: {err:?}"
            ))
        })?,
    })
}

/// `GET /data/v0/columns/{block_root}/{index}` — serve a single stored column.
/// 400 for an out-of-range index,
/// 404 when not held.
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

    Ok(HttpResponse::Ok()
        .content_type(ContentType::octet_stream())
        .body(column.payload().to_vec()))
}

/// `GET /data/v0/columns/{block_root}` — serve every column this node holds for
/// a block.
/// An unknown block yields an empty list, not a 404.
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
            columns.push(wire_entry(index, column.payload())?);
        }
    }

    // The list bound mirrors `NUMBER_OF_COLUMNS`, which also bounds the held
    // indices, so this only fails on a store that broke its own invariant.
    let batch: WireBlockBatch = VariableList::new(columns).map_err(|err| {
        ApiError::InternalError(format!("held columns exceed the batch bound: {err:?}"))
    })?;

    Ok(HttpResponse::Ok()
        .content_type(ContentType::octet_stream())
        .body(batch.as_ssz_bytes()))
}
