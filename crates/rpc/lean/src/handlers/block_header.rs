use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_chain_lean::lean_chain::LeanChainReader;
use ream_consensus_lean::block::BlockHeader;

use super::block::get_block_by_id;

// GET /lean/v0/headers/{block_id}
#[get("/headers/{block_id}")]
pub async fn get_block_header(
    block_id: Path<ID>,
    lean_chain: Data<LeanChainReader>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(BlockHeader::from(
        get_block_by_id(block_id.into_inner(), lean_chain)
            .await?
            .ok_or_else(|| ApiError::NotFound("Block not found".to_string()))?,
    )))
}
