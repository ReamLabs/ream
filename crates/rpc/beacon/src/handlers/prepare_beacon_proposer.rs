use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, Json},
};
use ream_api_types_beacon::request::PrepareBeaconProposerItem;
use ream_api_types_common::error::ApiError;
use ream_fork_choice::store::Store;
use ream_operation_pool::OperationPool;
use ream_storage::db::beacon::BeaconDB;

#[post("/validator/prepare_beacon_proposer")]
pub async fn prepare_beacon_proposer(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    prepare_beacon_proposer_items: Json<Vec<PrepareBeaconProposerItem>>,
) -> Result<impl Responder, ApiError> {
    let items = prepare_beacon_proposer_items.into_inner();

    if items.is_empty() {
        return Err(ApiError::BadRequest("Empty request body".to_string()));
    }

    // Create a store instance to get the current epoch
    let store = Store::new(db.get_ref().clone(), operation_pool.get_ref().clone());
    let current_epoch = store
        .get_current_store_epoch()
        .map_err(|err| ApiError::InternalError(format!("Failed to get current epoch: {err}")))?;

    for item in items {
        operation_pool.insert_proposer_preparation(
            item.validator_index,
            item.fee_recipient,
            current_epoch,
        );
    }

    Ok(HttpResponse::Ok().body("Preparation information has been received."))
}
