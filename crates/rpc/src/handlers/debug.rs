use std::{collections::HashSet, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use hashbrown::HashMap;
use ream_beacon_api_types::{
    error::ApiError,
    id::ID,
    responses::{BeaconHeadResponse, BeaconResponse, DataResponse, ForkChoiceResponse},
};
use ream_fork_choice::store::Store;
use ream_operation_pool::OperationPool;
use ream_storage::{db::ReamDB, tables::Field};

use crate::handlers::state::get_state_from_id;

#[get("/debug/beacon/states/{state_id}")]
pub async fn get_beacon_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconResponse::new(state)))
}

#[get("/debug/beacon/heads")]
pub async fn get_beacon_heads(db: Data<ReamDB>) -> Result<impl Responder, ApiError> {
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

    for block in blocks.values() {
        referenced_parents.insert(block.parent_root);
    }

    for (block_root, block) in &blocks {
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
pub async fn get_fork_choice(db: Data<ReamDB>) -> Result<impl Responder, ApiError> {
    let justified_checkpoint = todo!();
    let finalized_checkpoint = todo!();
    let fork_choice_nodes = vec![];

    Ok(HttpResponse::Ok().json(ForkChoiceResponse::new(
        justified_checkpoint,
        finalized_checkpoint,
        fork_choice_nodes,
    )))
}
