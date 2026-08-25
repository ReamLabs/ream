use actix_web::{
    HttpResponse, Responder, post,
    web::{self, Data},
};
use alloy_primitives::B256;
use ream_api_types_common::error::ApiError;
use ream_da::{
    column::{CandidateColumn, ColumnContext},
    id::ColumnId,
};
use ream_da_node::{error::IngestionError, ingest::IngestHandle};
use serde::Deserialize;

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
    handle.try_submit(candidate).map_err(|err| match err {
        IngestionError::Overloaded => {
            ApiError::ServiceUnavailable("verification queue is full; retry shortly".to_string())
        }
        IngestionError::Closed => {
            ApiError::InternalError("verification service is unavailable".to_string())
        }
    })?;
    Ok(HttpResponse::Accepted().finish())
}
