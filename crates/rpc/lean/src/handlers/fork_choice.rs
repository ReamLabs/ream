use actix_web::{
    HttpRequest, HttpResponse, Responder, get,
    http::header,
    web::{Data, Path},
};
use ream_api_types_common::{
    content_type::{ContentType, JSON_CONTENT_TYPE, SSZ_CONTENT_TYPE},
    error::ApiError,
    id::ID,
};
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::{beacon::justified_checkpoint, field::REDBField, lean::safe_target, table::REDBTable};
use ssz::Encode;


#[get("/fork_choice")]
pub async fn get_fork_choice_tree(
    lean_chain: Data<LeanStoreReader>
) -> Result<impl Responder, ApiError> {
    let lean_chain = lean_chain.read().await;
    let db = lean_chain.store.lock().await;

    let finalized_checkpoint = db.latest_finalized_provider()
        .get()
        .map_err(|err| {
            ApiError::InternalError(format!("Unable to get latest finalized checkpoint: {err:?}"))
        })?;
    
    let finalized_root = finalized_checkpoint.root;
    let finalized_slot = finalized_checkpoint.slot;


    let safe_target = db.safe_target_provider()
        .get()
        .map_err(|err| {
            ApiError::InternalError(format!("Unable to get safe target: {err:?}"))
        })?;

    
    let justified_checkpoint = db.latest_justified_provider()
        .get()
        .map_err(|err| {
            ApiError::InternalError(format!("Unable to get latest justified checkpoint: {err:?}"))
        })?;
    
    let justified_root = justified_checkpoint.root;
    let justified_slot = justified_checkpoint.slot;

    let head_root = db.head_provider()
        .get()
        .map_err(|err| {
            ApiError::InternalError(format!("Unable to get latest head root: {err:?}"))
        })?;
    
    let head_state = db
        .state_provider()
        .get(head_root)
        .map_err(|err| {
            ApiError::InternalError(format!("Unable to get head state: {err:?}"))
        })?;

    let validator_count = head_state.map(|state| state.validators.len() as u64).unwrap_or(0);

    let block_map = db.block_provider().get_children_map(0, attestation_weights);

    let blocks = db.block_provider().iter()
        .filter(|(_, block)| block.slot >= finalized_slot)
        .map(|(root, block)| {
            serde_json::json!({
                "root": format!("0x{}", hex::encode(root)),
                "slot": block.slot,
                "parent_root": format!("0x{}", hex::encode(&block.parent_root)),
                "proposer_index": block.proposer_index,
            })
        })
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "finalized_root": format!("0x{}", hex::encode(finalized_root)),
    })))
}