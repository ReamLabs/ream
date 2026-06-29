use actix_web::{
    HttpResponse, Responder, post,
    web::{self, Data},
};
use alloy_primitives::B256;
use ream_api_types_common::error::ApiError;
use ream_da::{
    column::{CandidateColumn, DaContext, DaPayload},
    id::DaColumnId,
};
use ream_da_node::{error::IngestionError, ingest::DaIngestHandle};
use serde::Deserialize;

/// JSON body of `POST /da/v0/ingest`.
///
/// The column payload stays opaque to the DA node; it travels as a hex string so
/// the envelope can be plain JSON for now (SSZ can replace this later).
#[derive(Deserialize)]
pub struct IngestRequest {
    block_root: B256,
    index: u64,
    slot: u64,
    payload: String,
}

impl IngestRequest {
    /// Decode and validate the request into a [`CandidateColumn`].
    fn into_candidate(self) -> Result<CandidateColumn, ApiError> {
        let id = DaColumnId::new(self.block_root, self.index)
            .map_err(|err| ApiError::BadRequest(format!("invalid column id: {err}")))?;
        let payload = alloy_primitives::hex::decode(&self.payload)
            .map_err(|err| ApiError::BadRequest(format!("payload is not valid hex: {err}")))?;
        Ok(CandidateColumn {
            id,
            context: DaContext { slot: self.slot },
            payload: DaPayload::new(payload),
        })
    }
}

/// `POST /da/v0/ingest` — admit a candidate column into the verification pipeline.
///
/// Uses the non-blocking [`DaIngestHandle::try_submit`] so a full queue sheds
/// load instead of blocking the request. The handler performs no verification;
/// it only decodes, validates the envelope, and hands the candidate off.
#[post("/ingest")]
pub async fn post_ingest(
    handle: Data<DaIngestHandle>,
    body: web::Json<IngestRequest>,
) -> Result<impl Responder, ApiError> {
    let candidate = body.into_inner().into_candidate()?;
    handle.try_submit(candidate).map_err(|err| match err {
        // Both are "the node could not accept this", not a client error.
        // TODO: a dedicated retryable 503 would fit `Overloaded` better than 500
        // once `ApiError` grows one.
        IngestionError::Overloaded => {
            ApiError::InternalError("verification queue is full; retry shortly".to_string())
        }
        IngestionError::Closed => {
            ApiError::InternalError("verification service is unavailable".to_string())
        }
    })?;
    Ok(HttpResponse::Accepted().finish())
}
