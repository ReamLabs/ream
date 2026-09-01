//! Integration tests for the data-availability RPC surface.

use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use actix_web::{
    App,
    body::MessageBody,
    dev::ServiceResponse,
    http::{StatusCode, header},
    test,
    web::Data,
};
use alloy_primitives::B256;
use ream_data_availability::{
    column::{ColumnContext, VerifiedColumn},
    id::ColumnId,
    store::{ColumnReadStore, ColumnWriteStore},
};
use ream_data_availability_node::{
    ingest::{IngestWorkItem, ingest_channel},
    store::FileColumnStore,
};
use serde_json::{Value, json};
use ssz::{Decode, Encode};
use ssz_types::VariableList;

use crate::{
    handlers::ingest::{WireBlockBatch, WireIndexedPayload},
    routes::register_routers,
};

/// A temp-dir-backed store that cleans up on drop.
struct TempStore {
    inner: Arc<FileColumnStore>,
    root: PathBuf,
}

impl TempStore {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ream-rpc-data-availabilityta-availabilityta-test-{}-{n}",
            std::process::id()
        ));
        let inner = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        Self { inner, root }
    }

    fn put(&self, block_root: B256, index: u64, slot: u64, payload: &[u8]) {
        let id = ColumnId::new(block_root, index).expect("valid index");
        self.inner
            .put(VerifiedColumn::new_unchecked(
                id,
                ColumnContext { slot },
                payload.to_vec(),
            ))
            .expect("put");
    }

    fn read_handle(&self) -> Arc<dyn ColumnReadStore> {
        self.inner.clone()
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).ok();
    }
}

// ---------------------------------------------------------------------------
// /health
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn health_reports_ok() {
    // No app_data needed: the probe touches neither store nor ingest handle.
    let app = test::init_service(App::new().configure(register_routers)).await;

    let req = test::TestRequest::get().uri("/data/v0/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"].as_str(), Some("healthy"));
    assert_eq!(body["service"].as_str(), Some("data-availability-node"));
}

// ---------------------------------------------------------------------------
// /ingest
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn ingest_accepts_valid_candidate() {
    let (handle, mut rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(1);
    let req = test::TestRequest::post()
        .uri("/data/v0/ingest")
        .set_json(json!({
            "block_root": format!("0x{root:x}"),
            "index": 3,
            "slot": 42,
            "payload": "0xdeadbeef",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    match rx.try_recv().expect("a candidate was enqueued") {
        IngestWorkItem::Candidate(candidate) => {
            assert_eq!(candidate.id.block_root(), root);
            assert_eq!(candidate.id.index(), 3);
            assert_eq!(candidate.context.slot, 42);
            assert_eq!(candidate.payload, [0xde, 0xad, 0xbe, 0xef]);
        }
        other => panic!("expected a candidate, got {other:?}"),
    }
}

#[actix_web::test]
async fn ingest_full_queue_is_503() {
    // Capacity 1 and `_rx` is never drained, so the second submit finds the
    // queue full.
    let (handle, _rx) = ingest_channel(1);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(1);
    let make_req = || {
        test::TestRequest::post()
            .uri("/data/v0/ingest")
            .set_json(json!({
                "block_root": format!("0x{root:x}"),
                "index": 3,
                "slot": 42,
                "payload": "0x00",
            }))
            .to_request()
    };

    let first = test::call_service(&app, make_req()).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let second = test::call_service(&app, make_req()).await;
    assert_eq!(second.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[actix_web::test]
async fn ingest_rejects_out_of_range_index() {
    let (handle, _rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(1);
    let req = test::TestRequest::post()
        .uri("/data/v0/ingest")
        .set_json(json!({
            "block_root": format!("0x{root:x}"),
            "index": 128, // == NUMBER_OF_COLUMNS, never a valid column
            "slot": 1,
            "payload": "0x00",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// /ingest/block/{block_root}
// ---------------------------------------------------------------------------

/// SSZ-encode a batch body from `(index, payload)` pairs.
fn batch_body(entries: &[(u64, &[u8])]) -> Vec<u8> {
    let entries: Vec<WireIndexedPayload> = entries
        .iter()
        .map(|(index, payload)| WireIndexedPayload {
            index: *index,
            payload: VariableList::new(payload.to_vec()).expect("payload within bound"),
        })
        .collect();
    let batch: WireBlockBatch = VariableList::new(entries).expect("batch within bound");
    batch.as_ssz_bytes()
}

#[actix_web::test]
async fn ingest_block_accepts_an_ssz_batch() {
    let (handle, mut rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(7);
    let req = test::TestRequest::post()
        .uri(&format!("/data/v0/ingest/block/0x{root:x}?slot=42"))
        .insert_header(("content-type", "application/octet-stream"))
        .set_payload(batch_body(&[(0, &[0xaa]), (5, &[0xbb]), (127, &[0xcc])]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    // The whole batch landed on the queue as one item, decoded verbatim.
    match rx.try_recv().expect("a batch was enqueued") {
        IngestWorkItem::CandidateBlock(block) => {
            assert_eq!(block.block_root(), root);
            assert_eq!(block.context().slot, 42);
            assert_eq!(block.columns_len(), 3);
            let expected = [(0, vec![0xaa]), (5, vec![0xbb]), (127, vec![0xcc])];
            for ((index, payload), (expected_index, expected_bytes)) in
                block.columns().iter().zip(&expected)
            {
                assert_eq!(index, expected_index);
                assert_eq!(payload, expected_bytes);
            }
        }
        other => panic!("expected a block batch, got {other:?}"),
    }
}

#[actix_web::test]
async fn ingest_block_rejects_a_non_ssz_content_type() {
    let (handle, _rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(7);
    let req = test::TestRequest::post()
        .uri(&format!("/data/v0/ingest/block/0x{root:x}?slot=1"))
        .insert_header(("content-type", "application/json"))
        .set_payload(batch_body(&[(0, &[0x00])]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[actix_web::test]
async fn ingest_block_rejects_a_malformed_ssz_body() {
    let (handle, _rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(7);
    let req = test::TestRequest::post()
        .uri(&format!("/data/v0/ingest/block/0x{root:x}?slot=1"))
        .insert_header(("content-type", "application/octet-stream"))
        // A bare offset pointing nowhere: not a decodable batch.
        .set_payload(vec![0xff, 0x00, 0x01])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn ingest_block_rejects_a_duplicate_column_index() {
    let (handle, _rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(7);
    let req = test::TestRequest::post()
        .uri(&format!("/data/v0/ingest/block/0x{root:x}?slot=1"))
        .insert_header(("content-type", "application/octet-stream"))
        .set_payload(batch_body(&[(3, &[0x00]), (3, &[0x01])]))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// /availability/{block_root}
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn availability_reports_held_and_missing() {
    let store = TempStore::new();
    let root = B256::repeat_byte(2);
    store.put(root, 0, 10, b"a");
    store.put(root, 2, 10, b"b");

    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/data/v0/availability/0x{root:x}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["complete"].as_bool(), Some(false));
    assert_eq!(body["held_count"].as_u64(), Some(2));
    let missing = body["missing"].as_array().expect("missing is an array");
    assert!(missing.iter().any(|v| v.as_u64() == Some(1)));
    assert!(!missing.iter().any(|v| v.as_u64() == Some(0)));
}

#[actix_web::test]
async fn availability_unknown_block_is_empty() {
    let store = TempStore::new();
    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    let unknown = B256::repeat_byte(9);
    let req = test::TestRequest::get()
        .uri(&format!("/data/v0/availability/0x{unknown:x}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["held_count"].as_u64(), Some(0));
    assert_eq!(body["complete"].as_bool(), Some(false));
}

#[actix_web::test]
async fn availability_rejects_non_root_id() {
    let store = TempStore::new();
    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/data/v0/availability/head")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// /columns/{block_root}[/{index}]
// ---------------------------------------------------------------------------

/// Assert a response carries the SSZ/raw-bytes content type.
fn assert_octet_stream(resp: &ServiceResponse<impl MessageBody>) {
    assert_eq!(
        resp.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/octet-stream"),
    );
}

#[actix_web::test]
async fn get_column_returns_stored_payload() {
    let store = TempStore::new();
    let root = B256::repeat_byte(3);
    store.put(root, 5, 77, &[0xde, 0xad, 0xbe, 0xef]);

    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/data/v0/columns/0x{root:x}/5"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_octet_stream(&resp);

    // The payload comes back verbatim: no SSZ wrapper, no hex, no metadata.
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), &[0xde, 0xad, 0xbe, 0xef]);
}

#[actix_web::test]
async fn get_column_absent_is_404() {
    let store = TempStore::new();
    let root = B256::repeat_byte(3);
    store.put(root, 5, 77, b"present");

    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    // Valid index, just not held for this block.
    let req = test::TestRequest::get()
        .uri(&format!("/data/v0/columns/0x{root:x}/6"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn get_column_out_of_range_index_is_400() {
    let store = TempStore::new();
    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    let root = B256::repeat_byte(3);
    let req = test::TestRequest::get()
        .uri(&format!("/data/v0/columns/0x{root:x}/999"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn get_columns_returns_every_held_column() {
    let store = TempStore::new();
    let root = B256::repeat_byte(4);
    let payloads = [(0u64, &[0xaa][..]), (1, &[0xbb, 0xbb]), (2, &[0xcc])];
    for (index, payload) in payloads {
        store.put(root, index, 30, payload);
    }

    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/data/v0/columns/0x{root:x}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_octet_stream(&resp);

    // The body is the same batch shape the ingest endpoint accepts, in
    // ascending column-index order.
    let body = test::read_body(resp).await;
    let batch = WireBlockBatch::from_ssz_bytes(&body).expect("a decodable batch");
    assert_eq!(batch.len(), 3);
    for (entry, (expected_index, expected_payload)) in batch.iter().zip(&payloads) {
        assert_eq!(entry.index, *expected_index);
        assert_eq!(entry.payload.as_ref(), *expected_payload);
    }
}

#[actix_web::test]
async fn get_columns_unknown_block_is_an_empty_batch() {
    let store = TempStore::new();
    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    // An unknown block is "nothing held", not an error.
    let unknown = B256::repeat_byte(9);
    let req = test::TestRequest::get()
        .uri(&format!("/data/v0/columns/0x{unknown:x}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_octet_stream(&resp);

    let body = test::read_body(resp).await;
    let batch = WireBlockBatch::from_ssz_bytes(&body).expect("a decodable batch");
    assert!(batch.is_empty());
}

// ---------------------------------------------------------------------------
// /retention/{slot}
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn retention_enqueues_hint() {
    let (handle, mut rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/data/v0/retention/99")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    match rx.try_recv().expect("a retention hint was enqueued") {
        IngestWorkItem::Retention(hint) => assert_eq!(hint.slot, 99),
        other => panic!("expected a retention hint, got {other:?}"),
    }
}

#[actix_web::test]
async fn retention_rejects_non_slot_id() {
    let (handle, _rx) = ingest_channel(8);
    let app = test::init_service(
        App::new()
            .app_data(Data::new(handle))
            .configure(register_routers),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/data/v0/retention/head")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
