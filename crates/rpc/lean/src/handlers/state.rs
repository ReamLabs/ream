use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_chain_lean::lean_chain::LeanChainReader;
use ream_consensus_lean::state::LeanState;
use tree_hash::TreeHash;

// GET /lean/v0/states/{block_id}
#[get("/states/{block_id}")]
pub async fn get_state(
    block_id: Path<ID>,
    lean_chain: Data<LeanChainReader>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(
        get_state_by_block_id(block_id.into_inner(), lean_chain)
            .await?
            .ok_or_else(|| ApiError::NotFound("Block not found".to_string()))?,
    ))
}

// Retrieve a state from the lean chain by its block ID.
pub async fn get_state_by_block_id(
    block_id: ID,
    lean_chain: Data<LeanChainReader>,
) -> Result<Option<LeanState>, ApiError> {
    // Obtain read guard first from the reader.
    let lean_chain = lean_chain.read().await;

    Ok(match block_id {
        ID::Finalized => {
            lean_chain.get_state_by_block_hash(lean_chain.latest_finalized_hash().ok_or(
                ApiError::InternalError("Failed to get latest finalized hash".to_string()),
            )?)
        }
        ID::Genesis => lean_chain.get_state_by_block_hash(lean_chain.genesis_hash),
        ID::Head => lean_chain.get_state_by_block_hash(lean_chain.head),
        ID::Justified => {
            lean_chain.get_state_by_block_hash(lean_chain.latest_justified_hash().ok_or(
                ApiError::InternalError("Failed to get latest justified hash".to_string()),
            )?)
        }
        ID::Slot(slot) => {
            let block = lean_chain
                .get_block_by_slot(slot)
                .ok_or(ApiError::NotFound("Block not found".to_string()))?;

            lean_chain.get_state_by_block_hash(block.tree_hash_root())
        }
        ID::Root(root) => lean_chain.get_state_by_block_hash(root),
    })
}
