use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_chain_lean::lean_chain::LeanChainReader;
use ream_storage::tables::{field::Field, table::Table};

// GET /lean/v0/states/{state_id}
#[get("/states/{state_id}")]
pub async fn get_state(
    state_id: Path<ID>,
    lean_chain: Data<LeanChainReader>,
) -> Result<impl Responder, ApiError> {
    let lean_chain = lean_chain.read().await;

    let block_root = match state_id.into_inner() {
        ID::Finalized => {
            let db = lean_chain.store.lock().await;
            Ok(db
                .latest_finalized_provider()
                .get()
                .map_err(|err| {
                    ApiError::InternalError(format!("No latest finalized hash: {err:?}"))
                })?
                .root)
        }
        ID::Genesis => Ok(lean_chain.genesis_hash),
        ID::Head => Ok(lean_chain.head),
        ID::Justified => {
            let db = lean_chain.store.lock().await;
            Ok(db
                .latest_justified_provider()
                .get()
                .map_err(|err| {
                    ApiError::InternalError(format!("No latest justified hash: {err:?}"))
                })?
                .root)
        }
        ID::Slot(slot) => lean_chain
            .get_block_id_by_slot(slot)
            .await
            .map_err(|err| ApiError::InternalError(format!("No block for slot {slot}: {err:?}"))),
        ID::Root(root) => {
            let provider = lean_chain.store.lock().await.state_root_index_provider();

            provider
                .get(root)
                .map_err(|err| ApiError::InternalError(format!("DB error: {err}")))?
                .ok_or_else(|| {
                    ApiError::NotFound(format!("Block ID not found for state root: {root:?}"))
                })
        }
    };

    let provider = lean_chain.store.clone().lock().await.lean_state_provider();

    Ok(HttpResponse::Ok().json(
        provider
            .get(block_root?)
            .map_err(|err| ApiError::InternalError(format!("DB error: {err}")))?
            .ok_or_else(|| ApiError::NotFound("Lean state not found".to_string()))?,
    ))
}
