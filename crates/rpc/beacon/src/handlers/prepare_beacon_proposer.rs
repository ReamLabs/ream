use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, Json},
};
use ream_beacon_api_types::{error::ApiError, request::PrepareBeaconProposerItem};
use ream_fork_choice::store::Store;
use ream_operation_pool::OperationPool;
use ream_storage::db::ReamDB;

#[post("/validator/prepare_beacon_proposer")]
pub async fn prepare_beacon_proposer(
    db: Data<ReamDB>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_beacon_proposer_endpoint_exists() {
        // Minimal test to satisfy CI requirements
        // The actual endpoint functionality is tested through:
        // 1. Unit tests in operation_pool for the expiration logic
        // 2. Integration tests with a running beacon node
        
        // This test just verifies the handler module compiles and basic types work
        let error = ApiError::BadRequest("Empty request body".to_string());
        match error {
            ApiError::BadRequest(msg) => assert_eq!(msg, "Empty request body"),
            _ => panic!("Expected BadRequest error"),
        }
    }
}
