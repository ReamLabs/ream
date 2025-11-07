use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_chain_lean::lean_chain::LeanChainReader;
use ream_consensus_lean::block::Block;
use ream_storage::tables::{field::Field, table::Table};

// GET /lean/v0/blocks/{block_id}
#[get("/blocks/{block_id}")]
pub async fn get_block(
    block_id: Path<ID>,
    lean_chain: Data<LeanChainReader>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(
        get_block_by_id(block_id.into_inner(), lean_chain)
            .await?
            .ok_or_else(|| ApiError::NotFound("Block not found".to_string()))?,
    ))
}

// Retrieve a block from the lean chain by its block ID.
pub async fn get_block_by_id(
    block_id: ID,
    lean_chain: Data<LeanChainReader>,
) -> Result<Option<Block>, ApiError> {
    let lean_chain = lean_chain.read().await;
    let block_root = match block_id {
        ID::Finalized => lean_chain
            .store
            .lock()
            .await
            .latest_finalized_provider()
            .get()
            .map(|checkpoint| checkpoint.root)
            .map_err(|err| ApiError::InternalError(format!("No latest finalized hash: {err:?}"))),
        ID::Genesis => {
            return Err(ApiError::NotFound(
                "This ID type is currently not supported".to_string(),
            ));
        }
        ID::Head => lean_chain
            .store
            .lock()
            .await
            .lean_head_provider()
            .get()
            .map_err(|err| ApiError::InternalError(format!("Could not get head: {err:?}"))),
        ID::Justified => lean_chain
            .store
            .lock()
            .await
            .latest_justified_provider()
            .get()
            .map(|checkpoint| checkpoint.root)
            .map_err(|err| ApiError::InternalError(format!("No latest justified hash: {err:?}"))),
        ID::Slot(slot) => lean_chain
            .get_block_id_by_slot(slot)
            .await
            .map_err(|err| ApiError::InternalError(format!("No block for slot {slot}: {err:?}"))),
        ID::Root(root) => Ok(root),
    };

    let provider = lean_chain.store.clone().lock().await.lean_block_provider();
    provider
        .get(block_root?)
        .map(|maybe_signed_block| {
            maybe_signed_block.map(|signed_block| signed_block.message.block.clone())
        })
        .map_err(|err| ApiError::InternalError(format!("DB error: {err}")))
}
