use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_api_types_common::error::ApiError;
use ream_api_types_lean::head::Head;
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::field::REDBField;

// GET /lean/v0/head
#[get("/head")]
pub async fn get_head(lean_chain: Data<LeanStoreReader>) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(Head {
        head: lean_chain
            .read()
            .await
            .store
            .lock()
            .await
            .head_provider()
            .get()
            .map_err(|err| ApiError::InternalError(format!("Could not get head: {err:?}")))?,
    }))
}
