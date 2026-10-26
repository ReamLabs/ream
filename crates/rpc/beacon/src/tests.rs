use std::sync::Arc;

use actix_web::{
    App,
    http::{StatusCode, header},
    test,
    web::Data,
};
use ream_api_types_beacon::responses::{
    ACCEPT_PRIORITY, BeaconVersionedResponse, DataResponse, ETH_CONSENSUS_VERSION_HEADER,
    SSZ_CONTENT_TYPE, VERSION,
};
use ream_bls::BLSSignature;
use ream_consensus_beacon::{
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        blinded_beacon_block::SignedBlindedBeaconBlock,
    },
    proposer_slashing::ProposerSlashing,
    sync_committe_selection::SyncCommitteeSelection,
};
use ream_operation_pool::OperationPool;
use ream_storage::{db::ReamDB, tables::table::REDBTable};
use ssz::Encode;
use tempfile::tempdir;
use tree_hash::TreeHash;

use crate::routes::register_routers;

#[actix_web::test]
async fn test_get_proposer_slashings() {
    let operation_pool = Arc::new(OperationPool::default());
    let slashing = ProposerSlashing {
        signed_header_1: Default::default(),
        signed_header_2: Default::default(),
    };
    operation_pool.insert_proposer_slashing(slashing.clone());

    let app = test::init_service(
        App::new()
            .app_data(Data::new(operation_pool))
            .configure(register_routers),
    )
    .await;

    let request = test::TestRequest::get()
        .uri("/eth/v1/beacon/pool/proposer_slashings")
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: DataResponse<Vec<ProposerSlashing>> = test::read_body_json(response).await;
    assert_eq!(body.data, vec![slashing]);

    let request = test::TestRequest::get()
        .uri("/eth/v1/beacon/pool/prposer_slashings")
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_post_sync_committee_selections() {
    let app = test::init_service(App::new().configure(register_routers)).await;
    let selection = SyncCommitteeSelection {
        validator_index: 1,
        slot: 2,
        subcommittee_index: 3,
        selection_proof: BLSSignature::default(),
    };

    let request = test::TestRequest::post()
        .uri("/eth/v1/validator/sync_committee_selections")
        .set_json(vec![selection.clone()])
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);

    let request = test::TestRequest::post()
        .uri("/eth/v1/validator/sync_committee_selections")
        .set_json(selection)
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn test_get_blinded_block() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let db = ReamDB::new(temp_dir.path().to_path_buf())
        .expect("failed to init ream db")
        .init_beacon_db()
        .expect("failed to init beacon db");

    let block = SignedBeaconBlock {
        message: BeaconBlock::default(),
        signature: Default::default(),
    };
    let block_root = block.message.tree_hash_root();
    db.block_provider()
        .insert(block_root, block.clone())
        .expect("failed to insert block");
    let blinded_block = block.as_signed_blinded_beacon_block();

    let app = test::init_service(
        App::new()
            .app_data(Data::new(db))
            .configure(register_routers),
    )
    .await;
    let uri = format!("/eth/v1/beacon/blinded_blocks/{block_root}");

    let request = test::TestRequest::get().uri(&uri).to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(ETH_CONSENSUS_VERSION_HEADER)
            .unwrap(),
        VERSION
    );
    let body: BeaconVersionedResponse<SignedBlindedBeaconBlock> =
        test::read_body_json(response).await;
    assert_eq!(body.version, VERSION);
    assert_eq!(body.data, blinded_block);

    let request = test::TestRequest::get()
        .uri(&uri)
        .insert_header((header::ACCEPT, ACCEPT_PRIORITY))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        SSZ_CONTENT_TYPE
    );
    assert_eq!(
        response
            .headers()
            .get(ETH_CONSENSUS_VERSION_HEADER)
            .unwrap(),
        VERSION
    );
    let body = test::read_body(response).await;
    assert_eq!(body, blinded_block.as_ssz_bytes());

    let request = test::TestRequest::get()
        .uri(&format!("/eth/v1/beacon/blind_block/{block_root}"))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
