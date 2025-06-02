use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_beacon_api_types::{error::ApiError, responses::DataResponse, sync::SyncStatus};
use ream_fork_choice::store::Store;
use ream_storage::{
    db::ReamDB,
    tables::{Table},
};
use serde::{Deserialize, Serialize};
use tracing::error;

#[derive(Serialize, Deserialize, Default)]
pub struct Syncing {
    sync_status: SyncStatus,
}

impl Syncing {
    pub fn new(head_slot: u64, sync_distance: u64) -> Self {
        Self {
            sync_status: SyncStatus {
                head_slot,
                sync_distance,
                // TODO
                is_syncing: true,
                // TODO
                is_optimistic: true,
                // TODO
                el_offline: true,
            },
        }
    }
}

/// Called by `eth/v1/node/syncing` to get the Node Version.
#[get("/node/syncing")]
pub async fn get_syncing_status(db: Data<ReamDB>) -> Result<impl Responder, ApiError> {
    let store = Store {
        db: db.get_ref().clone(),
    };

    // get head_slot
    let head = store.get_head().map_err(|err| {
        error!("Failed to get current slot, error: {err:?}");
        ApiError::InternalError
    })?;
    let head_slot = match db.beacon_block_provider().get(head) {
        Ok(Some(block)) => block.message.slot,
        err => {
            error!("Failed to get head slot, error: {err:?}");
            return Err(ApiError::InternalError);
        }
    };

    // calculate sync_distance
    let current_slot = store.get_current_slot().map_err(|err| {
        error!("Failed to get current slot, error: {err:?}");
        ApiError::InternalError
    })?;

    let sync_distance = head_slot - current_slot;

    Ok(HttpResponse::Ok().json(DataResponse::new(Syncing::new(head_slot, sync_distance))))
}
