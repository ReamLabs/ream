use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, Json},
};
use ream_beacon_api_types::{error::ApiError, request::PrepareBeaconProposerItem};
use ream_operation_pool::OperationPool;

#[post("/validator/prepare_beacon_proposer")]
pub async fn prepare_beacon_proposer(
    operation_pool: Data<Arc<OperationPool>>,
    prepare_beacon_proposer_items: Json<Vec<PrepareBeaconProposerItem>>,
) -> Result<impl Responder, ApiError> {
    let items = prepare_beacon_proposer_items.into_inner();

    if items.is_empty() {
        return Err(ApiError::BadRequest("Empty request body".to_string()));
    }

    for item in items {
        operation_pool.insert_proposer_preparation(item.validator_index, item.fee_recipient);
    }

    Ok(HttpResponse::Ok().finish())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use actix_web::{App, http::StatusCode, test};
    use alloy_primitives::Address;

    use super::*;

    #[actix_web::test]
    async fn test_prepare_beacon_proposer_success() {
        let operation_pool = Arc::new(OperationPool::default());
        let app = test::init_service(
            App::new()
                .app_data(Data::new(operation_pool))
                .service(prepare_beacon_proposer),
        )
        .await;

        let fee_recipient = Address::from([0x42; 20]);
        let items = vec![
            PrepareBeaconProposerItem {
                validator_index: 1,
                fee_recipient,
            },
            PrepareBeaconProposerItem {
                validator_index: 2,
                fee_recipient,
            },
        ];

        let request = test::TestRequest::post()
            .uri("/validator/prepare_beacon_proposer")
            .set_json(&items)
            .to_request();

        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn test_prepare_beacon_proposer_empty_request() {
        let operation_pool = Arc::new(OperationPool::default());
        let app = test::init_service(
            App::new()
                .app_data(Data::new(operation_pool))
                .service(prepare_beacon_proposer),
        )
        .await;

        let items: Vec<PrepareBeaconProposerItem> = vec![];

        let request = test::TestRequest::post()
            .uri("/validator/prepare_beacon_proposer")
            .set_json(&items)
            .to_request();

        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn test_prepare_beacon_proposer_stores_data() {
        let operation_pool = Arc::new(OperationPool::default());
        let app = test::init_service(
            App::new()
                .app_data(Data::new(operation_pool.clone()))
                .service(prepare_beacon_proposer),
        )
        .await;

        let fee_recipient = Address::from([0x42; 20]);
        let items = vec![PrepareBeaconProposerItem {
            validator_index: 123,
            fee_recipient,
        }];

        let request = test::TestRequest::post()
            .uri("/validator/prepare_beacon_proposer")
            .set_json(&items)
            .to_request();

        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);

        assert_eq!(
            operation_pool.get_proposer_preparation(123),
            Some(fee_recipient)
        );
    }
}
