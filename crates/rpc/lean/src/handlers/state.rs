use actix_web::{
    HttpRequest, HttpResponse, Responder, get,
    http::header,
    web::{Data, Path},
};
use ream_api_types_common::{
    content_type::{ContentType, JSON_CONTENT_TYPE, SSZ_CONTENT_TYPE},
    error::ApiError,
    id::ID,
};
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::{field::REDBField, table::REDBTable};
use ssz::Encode;

// GET /lean/v0/states/{state_id}
#[get("/states/{state_id}")]
pub async fn get_state(
    http_request: HttpRequest,
    state_id: Path<ID>,
    lean_chain: Data<LeanStoreReader>,
) -> Result<impl Responder, ApiError> {
    let lean_chain = lean_chain.read().await;

    let block_root = match state_id.into_inner() {
        ID::Finalized => {
            let db = lean_chain.store.lock().await;
            Ok(db
                .latest_finalized_provider()
                .get()
                .map_err(|err| {
                    ApiError::InternalError(format!("No latest finalized hash: {err:?}"))
                })?
                .root)
        }
        ID::Genesis => {
            return Err(ApiError::NotFound(
                "This ID type is currently not supported".to_string(),
            ));
        }
        ID::Head => lean_chain
            .store
            .lock()
            .await
            .head_provider()
            .get()
            .map_err(|err| ApiError::InternalError(format!("Could not get head: {err:?}"))),
        ID::Justified => {
            let db = lean_chain.store.lock().await;
            Ok(db
                .latest_justified_provider()
                .get()
                .map_err(|err| {
                    ApiError::InternalError(format!("No latest justified hash: {err:?}"))
                })?
                .root)
        }
        ID::Slot(slot) => lean_chain
            .get_block_id_by_slot(slot)
            .await
            .map_err(|err| ApiError::InternalError(format!("No block for slot {slot}: {err:?}"))),
        ID::Root(root) => {
            let provider = lean_chain.store.lock().await.state_root_index_provider();

            provider
                .get(root)
                .map_err(|err| ApiError::InternalError(format!("DB error: {err}")))?
                .ok_or_else(|| {
                    ApiError::NotFound(format!("Block ID not found for state root: {root:?}"))
                })
        }
    };

    let db = lean_chain.store.lock().await;
    let state = db
        .state_provider()
        .get(block_root?)
        .map_err(|err| ApiError::InternalError(format!("DB error: {err}")))?
        .ok_or_else(|| ApiError::NotFound("Lean state not found".to_string()))?;

    match ContentType::from(http_request.headers().get(header::ACCEPT)) {
        ContentType::Ssz => Ok(HttpResponse::Ok()
            .content_type(SSZ_CONTENT_TYPE)
            .body(state.as_ssz_bytes())),
        ContentType::Json => Ok(HttpResponse::Ok()
            .content_type(JSON_CONTENT_TYPE)
            .json(state)),
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web::Data};
    use ream_consensus_lean::state::LeanState;
    use ream_sync::rwlock::Writer;
    use ream_test_utils::store::sample_store;
    use ssz::Decode;

    use super::get_state;

    #[tokio::test]
    async fn test_get_finalized_state_returns_ssz() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let app =
            test::init_service(App::new().app_data(Data::new(reader)).service(get_state)).await;

        let req = test::TestRequest::get()
            .uri("/states/finalized")
            .insert_header(("Accept", "application/octet-stream"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/octet-stream"
        );

        let body = test::read_body(resp).await;
        let state = LeanState::from_ssz_bytes(&body).expect("Failed to decode SSZ");
        assert!(!state.validators.is_empty());
    }

    #[tokio::test]
    async fn test_get_finalized_state_returns_json() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let app =
            test::init_service(App::new().app_data(Data::new(reader)).service(get_state)).await;

        let req = test::TestRequest::get()
            .uri("/states/finalized")
            .insert_header(("Accept", "application/json"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("application/json")
        );
    }

    #[tokio::test]
    async fn test_get_finalized_state_defaults_to_json() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let app =
            test::init_service(App::new().app_data(Data::new(reader)).service(get_state)).await;

        let req = test::TestRequest::get()
            .uri("/states/finalized")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("application/json")
        );
    }
}
