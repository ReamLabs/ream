// use actix_web::{
//     HttpResponse, Responder, get,
//     web::{Data, Path},
// };
// use ream_api_types_common::{error::ApiError, id::ID};
// use ream_chain_lean::lean_chain::LeanChainReader;
// use ream_storage::tables::table::Table;
// use ream_consensus_lean::block::Block;

// // GET /lean/v0/blocks/{block_id}
// #[get("/blocks/{block_id}")]
// pub async fn get_block(
//     block_id: Path<ID>,
//     lean_chain: Data<LeanChainReader>,
// ) -> Result<impl Responder, ApiError> {
//     Ok(HttpResponse::Ok().json(
//         get_block_by_id(block_id.into_inner(), lean_chain)
//             .await?
//             .ok_or_else(|| ApiError::NotFound("Block not found".to_string()))?,
//     ))
// }

// // Retrieve a block from the lean chain by its block ID.
// pub async fn get_block_by_id(
//     block_id: ID,
//     lean_chain: Data<LeanChainReader>,
// ) -> Result<Option<Block>, ApiError> {
//     // Obtain read guard first from the reader.
//     let lean_block_provider = {
//         let lean_chain = lean_chain.read().await;
//         lean_chain.store.lock().await.lean_block_provider()
//     };

//     Ok(match block_id {
//         ID::Finalized => lean_block_provider.get(lean_chain.latest_finalized_hash()?.ok_or(
//             ApiError::InternalError("Failed to get latest finalized hash".to_string()),
//         )?),
//         ID::Genesis => lean_chain.get_block_by_root(lean_chain.genesis_hash),
//         ID::Head => lean_chain.get_block_by_root(lean_chain.head),
//         ID::Justified => lean_chain.get_block_by_root(lean_chain.latest_justified_hash().ok_or(
//             ApiError::InternalError("Failed to get latest justified hash".to_string()),
//         )?),
//         ID::Slot(slot) => lean_chain.get_block_by_slot(slot),
//         ID::Root(root) => lean_chain.get_block_by_root(root),
//     })
// }
