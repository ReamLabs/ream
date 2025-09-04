use std::sync::Arc;

use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_api_types_beacon::{
    responses::{DataResponse, EXECUTION_OPTIMISTIC},
    sync::SyncStatus,
};
use ream_api_types_common::error::ApiError;
use ream_execution_engine::ExecutionEngine;
use ream_fork_choice::store::Store;
use ream_operation_pool::OperationPool;
use ream_storage::{db::beacon::BeaconDB, tables::table::Table};
use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Serialize, Deserialize, Default)]
pub struct Syncing {
    sync_status: SyncStatus,
}

impl Syncing {
    pub fn new(head_slot: u64, sync_distance: u64, el_offline: bool, is_syncing: bool) -> Self {
        Self {
            sync_status: SyncStatus {
                head_slot,
                sync_distance,
                is_syncing,
                el_offline,
                is_optimistic: EXECUTION_OPTIMISTIC,
            },
        }
    }
}

/// Called by `eth/v1/node/syncing` to get the Node Version.
#[get("/node/syncing")]
pub async fn get_syncing_status(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    execution_engine: Data<Option<ExecutionEngine>>,
) -> Result<impl Responder, ApiError> {
    let store = Store {
        db: db.get_ref().clone(),
        operation_pool: operation_pool.get_ref().clone(),
    };

    // get head_slot
    let head = store.get_head().map_err(|err| {
        ApiError::InternalError(format!("Failed to get current slot, error: {err:?}"))
    })?;

    let head_slot = match db.beacon_block_provider().get(head) {
        Ok(Some(block)) => block.message.slot,
        err => {
            return Err(ApiError::InternalError(format!(
                "Failed to get head slot, error: {err:?}"
            )));
        }
    };

    // calculate sync_distance
    let current_slot = store.get_current_slot().map_err(|err| {
        ApiError::InternalError(format!("Failed to get current slot, error: {err:?}"))
    })?;

    let sync_distance = current_slot.saturating_sub(head_slot);

    // get el_offline
    let el_offline = match &**execution_engine {
        Some(execution_engine) => match execution_engine.eth_chain_id().await {
            Ok(_) => false,
            Err(err) => {
                error!("Execution engine is offline or erroring, error: {err:?}");
                true
            }
        },
        None => true,
    };

    Ok(HttpResponse::Ok().json(DataResponse::new(Syncing::new(
        head_slot,
        sync_distance,
        el_offline,
        // get is_syncing
        sync_distance > 1,
    ))))
}
