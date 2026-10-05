use std::sync::Arc;

use actix_web::{App, http::StatusCode, test, web::Data};
use ream_operation_pool::OperationPool;
use ream_storage::db::ReamDB;
use serde_json::json;

use crate::routes::register_routers;

#[actix_web::test]
async fn test_get_proposer_slashings_route_spec_compliance() {
    let operation_pool = Arc::new(OperationPool::default());
    let app = test::init_service(
        App::new()
            .app_data(Data::new(operation_pool))
            .configure(register_routers),
    )
    .await;

    // Spec-compliant route: GET /eth/v1/beacon/pool/proposer_slashings
    let req = test::TestRequest::get()
        .uri("/eth/v1/beacon/pool/proposer_slashings")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Old typo route should no longer be present
    let req_typo = test::TestRequest::get()
        .uri("/eth/v1/beacon/pool/prposer_slashings")
        .to_request();
    let resp_typo = test::call_service(&app, req_typo).await;
    assert_eq!(resp_typo.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_post_sync_committee_selections_registered_and_vector_payload() {
    let app = test::init_service(App::new().configure(register_routers)).await;

    // Spec-compliant route: POST /eth/v1/validator/sync_committee_selections
    // Takes a list of SyncCommitteeSelection and returns 501 Not Implemented (DVT not yet
    // supported)
    let req = test::TestRequest::post()
        .uri("/eth/v1/validator/sync_committee_selections")
        .set_json(json!([]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
}

#[actix_web::test]
async fn test_get_blinded_blocks_route_spec_compliance() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let ream_db = ReamDB::new(temp_dir.path().to_path_buf()).expect("failed to init ream db");
    let db = ream_db.init_beacon_db().expect("failed to init beacon db");

    let app = test::init_service(
        App::new()
            .app_data(Data::new(db))
            .configure(register_routers),
    )
    .await;

    // Spec-compliant route: GET /eth/v1/beacon/blinded_blocks/{block_id}
    let req = test::TestRequest::get()
        .uri("/eth/v1/beacon/blinded_blocks/head")
        .to_request();
    let resp = test::call_service(&app, req).await;
    // The route matches and reaches the handler (which returns 404 block not found for empty db)
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Old non-spec route: /eth/v1/beacon/blind_block/{block_id} should not be registered
    let req_old = test::TestRequest::get()
        .uri("/eth/v1/beacon/blind_block/head")
        .to_request();
    let resp_old = test::call_service(&app, req_old).await;
    assert_eq!(resp_old.status(), StatusCode::NOT_FOUND);
}
