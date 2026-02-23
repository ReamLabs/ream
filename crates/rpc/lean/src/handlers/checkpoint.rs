use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_api_types_common::error::ApiError;
use ream_fork_choice_lean::store::LeanStoreReader;
use ream_storage::tables::field::REDBField;

// GET /lean/v0/checkpoints/justified
#[get("/checkpoints/justified")]
pub async fn get_justified_checkpoint(
    lean_chain: Data<LeanStoreReader>,
) -> Result<impl Responder, ApiError> {
    let checkpoint = lean_chain
        .read()
        .await
        .store
        .lock()
        .await
        .latest_justified_provider()
        .get()
        .map_err(|err| {
            ApiError::InternalError(format!("Could not get justified checkpoint: {err:?}"))
        })?;

    Ok(HttpResponse::Ok().json(checkpoint))
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web::Data};
    use ream_consensus_lean::checkpoint::Checkpoint;
    use ream_sync::rwlock::Writer;
    use ream_test_utils::sample_store;

    use super::get_justified_checkpoint;

    #[tokio::test]
    async fn test_get_justified_checkpoint_returns_json() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let app = test::init_service(
            App::new()
                .app_data(Data::new(reader))
                .service(get_justified_checkpoint),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/checkpoints/justified")
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

        let body = test::read_body(resp).await;
        let checkpoint: Checkpoint = serde_json::from_slice(&body).expect("Failed to decode JSON");
        assert_eq!(checkpoint.slot, 0);
    }
}
