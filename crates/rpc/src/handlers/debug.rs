use std::{collections::HashSet, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use hashbrown::HashMap;
use ream_beacon_api_types::{
    error::ApiError,
    id::ID,
    responses::{
        BeaconHeadResponse, BeaconResponse, DataResponse, ForkChoiceNode, ForkChoiceResponse,
        ForkChoiceValidity,
    },
};
use ream_fork_choice::store::Store;
use ream_operation_pool::OperationPool;
use ream_storage::{db::ReamDB, tables::Field};

use crate::handlers::state::get_state_from_id;

#[get("/debug/beacon/states/{state_id}")]
pub async fn get_debug_beacon_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconResponse::new(state)))
}

#[get("/debug/beacon/heads")]
pub async fn get_debug_beacon_heads(db: Data<ReamDB>) -> Result<impl Responder, ApiError> {
    let justified_checkpoint = db.justified_checkpoint_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to get justified_checkpoint, error: {err:?}"
        ))
    })?;

    let mut blocks = HashMap::new();
    let store = Store {
        db: db.get_ref().clone(),
        operation_pool: Arc::new(OperationPool::default()),
    };

    store
        .filter_block_tree(justified_checkpoint.root, &mut blocks)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to filter block tree, error: {err:?}"))
        })?;

    let mut leaves = vec![];
    let mut referenced_parents = HashSet::new();

    for (block, _, _) in blocks.values() {
        referenced_parents.insert(block.parent_root);
    }

    for (block_root, (block, _, _)) in &blocks {
        if !referenced_parents.contains(block_root) {
            leaves.push(BeaconHeadResponse {
                root: block.block_root(),
                slot: block.slot,
                execution_optimistic: false,
            });
        }
    }

    Ok(HttpResponse::Ok().json(DataResponse::new(leaves)))
}

#[get("/debug/fork_choice")]
pub async fn get_debug_fork_choice(db: Data<ReamDB>) -> Result<impl Responder, ApiError> {
    let justified_checkpoint = db.justified_checkpoint_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to get justified_checkpoint, error: {err:?}"
        ))
    })?;
    let finalized_checkpoint = db.finalized_checkpoint_provider().get().map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to get finalized_checkpoint, error: {err:?}"
        ))
    })?;

    let store = Store {
        db: db.get_ref().clone(),
        operation_pool: Arc::new(OperationPool::default()),
    };
    let blocks = store.get_filtered_block_tree().map_err(|err| {
        ApiError::InternalError(format!("Failed to get filtered block tree, error: {err:?}"))
    })?;
    let mut fork_choice_nodes = Vec::with_capacity(blocks.len());
    for (block_root, (block, justified_epoch, finalized_epoch)) in blocks {
        let weight = store.get_weight(block_root).map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to get weight for block {block_root:?}, error: {err:?}"
            ))
        })?;

        fork_choice_nodes.push(ForkChoiceNode {
            slot: block.slot,
            block_root,
            parent_root: block.parent_root,
            justified_epoch,
            finalized_epoch,
            weight,
            // NOTE: As `EXECUTION_OPTIMISTIC` is default to false, validity will be always "valid"
            // in this context.
            validity: ForkChoiceValidity::Valid,
            execution_block_hash: block.body.execution_payload.block_hash,
            extra_data: Default::default(),
        });
    }

    Ok(HttpResponse::Ok().json(ForkChoiceResponse::new(
        justified_checkpoint,
        finalized_checkpoint,
        fork_choice_nodes,
    )))
}
