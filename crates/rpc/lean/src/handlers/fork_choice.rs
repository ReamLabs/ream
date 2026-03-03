use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_api_types_common::error::ApiError;
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::{field::REDBField, table::REDBTable};
use tree_hash::TreeHash;

#[get("/fork_choice")]
pub async fn get_fork_choice_tree(
    lean_chain: Data<LeanStoreReader>,
) -> Result<impl Responder, ApiError> {
    let lean_chain = lean_chain.read().await;
    let db = lean_chain.store.lock().await;
    let weight_map = lean_chain.compute_block_weights().await;

    let finalized_checkpoint = db.latest_finalized_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Unable to get latest finalized checkpoint: {err:?}"
        ))
    })?;

    let finalized_root = finalized_checkpoint.root;
    let finalized_slot = finalized_checkpoint.slot;

    let safe_target = db
        .safe_target_provider()
        .get()
        .map_err(|err| ApiError::InternalError(format!("Unable to get safe target: {err:?}")))?;

    let justified_checkpoint = db.latest_justified_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Unable to get latest justified checkpoint: {err:?}"
        ))
    })?;

    let justified_root = justified_checkpoint.root;
    let justified_slot = justified_checkpoint.slot;

    let head_root = db.head_provider().get().map_err(|err| {
        ApiError::InternalError(format!("Unable to get latest head root: {err:?}"))
    })?;

    let head_state = db
        .state_provider()
        .get(head_root)
        .map_err(|err| ApiError::InternalError(format!("Unable to get head state: {err:?}")))?;

    let validator_count = head_state
        .map(|state| state.validators.len() as u64)
        .unwrap_or(0);

    let blocks = db
        .block_provider()
        .get_all_blocks(finalized_slot)
        .unwrap_or(vec![])
        .iter()
        .map(|block| {
            let root = block.message.block.tree_hash_root();
            let weight = weight_map
                .as_ref()
                .map(|weight_map| weight_map.get(&root).cloned().unwrap_or(0))
                .unwrap_or(0);
            serde_json::json!({
                "root": root,
                "slot": block.message.block.slot,
                "parent_root": block.message.block.parent_root,
                "proposer_index": block.message.block.proposer_index,
                "weight": weight,
            })
        })
        .collect::<Vec<_>>();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "nodes": blocks,
        "head": head_root,
        "justified": {
            "slot": justified_slot,
            "root": justified_root,
        },
        "finalized": {
            "slot": finalized_slot,
            "root": finalized_root,
        },
        "safe_target": safe_target,
        "validator_count": validator_count,
    })))
}
