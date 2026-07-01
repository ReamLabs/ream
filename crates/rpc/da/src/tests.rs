//! Integration tests for the DA RPC surface.

use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use actix_web::{App, http::StatusCode, test, web::Data};
use alloy_primitives::B256;
use ream_da::{
    column::{DaContext, DaPayload, VerifiedColumn},
    id::DaColumnId,
    store::{DaReadStore, DaWriteStore},
};
use ream_da_node::{
    ingest::{DaWorkItem, ingest_channel},
    store::DaFileStore,
};
use serde_json::{Value, json};

use crate::routes::register_routers;

/// A temp-dir-backed store that cleans up on drop, with a tiny `put` helper for
/// seeding columns.
struct TempStore {
    inner: Arc<DaFileStore>,
    root: PathBuf,
}

impl TempStore {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("ream-rpc-da-test-{}-{n}", std::process::id()));
        let inner = Arc::new(DaFileStore::new(root.clone()).expect("open store"));
        Self { inner, root }
    }

    fn put(&self, block_root: B256, index: u64, slot: u64, payload: &[u8]) {
        let id = DaColumnId::new(block_root, index).expect("valid index");
        self.inner
            .put(VerifiedColumn::new_unchecked(
                id,
                DaContext { slot },
                DaPayload::new(payload.to_vec()),
            ))
            .expect("put");
    }

    fn read_handle(&self) -> Arc<dyn DaReadStore> {
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

    let req = test::TestRequest::get()
        .uri("/da/v0/health")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"].as_str(), Some("healthy"));
    assert_eq!(body["service"].as_str(), Some("da-node"));
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
        .uri("/da/v0/ingest")
        .set_json(json!({
            "block_root": format!("0x{root:x}"),
            "index": 3,
            "slot": 42,
            "payload": "0xdeadbeef",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    // The candidate landed on the queue with its envelope decoded correctly.
    match rx.try_recv().expect("a candidate was enqueued") {
        DaWorkItem::Candidate(candidate) => {
            assert_eq!(candidate.id.block_root(), root);
            assert_eq!(candidate.id.index(), 3);
            assert_eq!(candidate.context.slot, 42);
            assert_eq!(candidate.payload.as_bytes(), &[0xde, 0xad, 0xbe, 0xef]);
        }
        other => panic!("expected a candidate, got {other:?}"),
    }
}

#[actix_web::test]
async fn ingest_full_queue_is_503() {
    // Capacity 1 and the receiver is never drained (`_rx` just keeps the channel
    // open), so the second submit finds the queue full — a retryable 503, not a
    // 500.
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
            .uri("/da/v0/ingest")
            .set_json(json!({
                "block_root": format!("0x{root:x}"),
                "index": 3,
                "slot": 42,
                "payload": "0x00",
            }))
            .to_request()
    };

    // First submit fills the single slot.
    let first = test::call_service(&app, make_req()).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    // Second finds the queue full.
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
        .uri("/da/v0/ingest")
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
        .uri(&format!("/da/v0/availability/0x{root:x}"))
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
        .uri(&format!("/da/v0/availability/0x{unknown:x}"))
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
        .uri("/da/v0/availability/head")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// /columns/{block_root}[/{index}]
// ---------------------------------------------------------------------------

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
        .uri(&format!("/da/v0/columns/0x{root:x}/5"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["index"].as_u64(), Some(5));
    assert_eq!(body["slot"].as_u64(), Some(77));
    assert_eq!(body["payload"].as_str(), Some("0xdeadbeef"));
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
        .uri(&format!("/da/v0/columns/0x{root:x}/6"))
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
        .uri(&format!("/da/v0/columns/0x{root:x}/999"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[actix_web::test]
async fn get_columns_returns_every_held_column() {
    let store = TempStore::new();
    let root = B256::repeat_byte(4);
    for index in [0u64, 1, 2] {
        store.put(root, index, 30, b"x");
    }

    let app = test::init_service(
        App::new()
            .app_data(Data::new(store.read_handle()))
            .configure(register_routers),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/da/v0/columns/0x{root:x}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    let columns = body.as_array().expect("an array of columns");
    assert_eq!(columns.len(), 3);
    let mut indices: Vec<u64> = columns
        .iter()
        .map(|c| c["index"].as_u64().expect("index"))
        .collect();
    indices.sort_unstable();
    assert_eq!(indices, vec![0, 1, 2]);
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
        .uri("/da/v0/retention/99")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    // The boundary rides the same queue as candidates, not the store directly.
    match rx.try_recv().expect("a retention hint was enqueued") {
        DaWorkItem::Retention(hint) => assert_eq!(hint.slot, 99),
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
        .uri("/da/v0/retention/head")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
