use actix_web::{
    HttpMessage, HttpRequest, HttpResponse, Responder, post,
    web::{self, Bytes, Data, Path, Query},
};
use alloy_primitives::B256;
use ream_api_types_common::{error::ApiError, id::ID};
use ream_data_availability::{
    column::{CandidateBlock, CandidateColumn, ColumnContext},
    id::ColumnId,
};
use ream_data_availability_node::ingest::IngestHandle;
use serde::Deserialize;
use ssz::Decode;
use ssz_derive::{Decode as SszDecode, Encode as SszEncode};
use ssz_types::{VariableList, typenum};

use crate::handlers::{block_root_from_id, submission_error};

/// Byte ceiling for a batch-ingest request body.
///
/// Size arithmetic: each blob contributes 2144 B to a column sidecar (one
/// 2048-B cell + 48-B commitment + 48-B proof), plus ~356 B of fixed fields.
/// At today's 21-blob schedule cap one sidecar is ~44 KB, so a whole
/// 128-column block is ~5.7 MB; a future 48-blob cap (~13 MB) still fits.
/// 32 MiB buys that headroom without revisiting this constant every fork.
pub const MAX_BATCH_INGEST_BODY_BYTES: usize = 1 << 25; // 32 MiB

/// JSON body of `POST /data/v0/ingest`; the payload travels as a hex string.
#[derive(Deserialize)]
pub struct IngestRequest {
    block_root: B256,
    index: u64,
    slot: u64,
    payload: String,
}

impl IngestRequest {
    fn into_candidate(self) -> Result<CandidateColumn, ApiError> {
        let id = ColumnId::new(self.block_root, self.index)
            .map_err(|err| ApiError::BadRequest(format!("invalid column id: {err}")))?;
        let payload = alloy_primitives::hex::decode(&self.payload)
            .map_err(|err| ApiError::BadRequest(format!("payload is not valid hex: {err}")))?;
        Ok(CandidateColumn {
            id,
            context: ColumnContext { slot: self.slot },
            payload,
        })
    }
}

/// `POST /data/v0/ingest` — admit a candidate column into the verification
/// pipeline. The handler performs no verification itself.
#[post("/ingest")]
pub async fn post_ingest(
    handle: Data<IngestHandle>,
    body: web::Json<IngestRequest>,
) -> Result<impl Responder, ApiError> {
    let candidate = body.into_inner().into_candidate()?;
    handle.try_submit(candidate).map_err(submission_error)?;
    Ok(HttpResponse::Accepted().finish())
}

/// One `(column index, payload)` entry of the SSZ batch-ingest body. The
/// payload stays opaque here, exactly as in the JSON envelope.
#[derive(Debug, Clone, PartialEq, Eq, SszEncode, SszDecode)]
pub struct WireIndexedPayload {
    pub index: u64,
    /// Opaque column payload; the bound is a per-column byte ceiling, not a
    /// scheme statement. Even at the SSZ spec's theoretical 4096-blob maximum
    /// a sidecar is only ~8.8 MB (4096 × 2144 B), so 16 MiB never rejects a
    /// legitimate column while still refusing absurd length claims.
    pub payload: VariableList<u8, typenum::U16777216>,
}

/// SSZ body of `POST /data/v0/ingest/block/{block_root}`: at most one payload
/// per column of one block (the list bound mirrors `NUMBER_OF_COLUMNS`).
pub type WireBlockBatch = VariableList<WireIndexedPayload, typenum::U128>;

/// Query string of `POST /data/v0/ingest/block/{block_root}`.
#[derive(Deserialize)]
pub struct BlockIngestQuery {
    /// Slot of the block the columns belong to — consensus context the node
    /// cannot read out of the opaque payloads itself.
    slot: u64,
}

/// `POST /data/v0/ingest/block/{block_root}?slot={slot}` — admit a whole block's
/// columns in one request.
///
/// The body is SSZ over `application/octet-stream`: a list of
/// `(column index, payload)` entries, payloads verbatim — no hex, no JSON.
/// Metadata the node cannot derive from opaque payloads (block root, slot)
/// travels in the URL. The batch is queued or refused as one unit, through the
/// same load-shedding [`IngestHandle::try_submit_block`] path as the
/// single-column endpoint.
#[post("/ingest/block/{block_root}")]
pub async fn post_ingest_block(
    handle: Data<IngestHandle>,
    block_root: Path<ID>,
    query: Query<BlockIngestQuery>,
    request: HttpRequest,
    body: Bytes,
) -> Result<impl Responder, ApiError> {
    if request.content_type() != "application/octet-stream" {
        return Err(ApiError::UnsupportedMediaType(format!(
            "batch ingest takes an SSZ body as application/octet-stream, got `{}`",
            request.content_type()
        )));
    }

    let block_root = block_root_from_id(block_root.into_inner())?;
    let entries = WireBlockBatch::from_ssz_bytes(&body)
        .map_err(|err| ApiError::BadRequest(format!("body is not a valid SSZ batch: {err:?}")))?;

    let columns = entries
        .into_iter()
        .map(|entry| (entry.index, entry.payload.to_vec()))
        .collect();
    // CandidateBlock::new re-checks the batch shape (empty, range, duplicates),
    // so a malformed batch dies here with a 400 instead of inside the pipeline.
    let block = CandidateBlock::new(block_root, ColumnContext { slot: query.slot }, columns)
        .map_err(|err| ApiError::BadRequest(format!("invalid batch: {err}")))?;

    handle.try_submit_block(block).map_err(submission_error)?;
    Ok(HttpResponse::Accepted().finish())
}
