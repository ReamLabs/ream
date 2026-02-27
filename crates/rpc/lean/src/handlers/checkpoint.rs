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
    use std::net::TcpListener;

    use actix_web::{
        App, HttpServer,
        http::StatusCode,
        test,
        web::{Data, scope},
    };
    use ream_checkpoint_sync_lean::{LeanCheckpointClient, verify_checkpoint_state};
    use ream_consensus_lean::{checkpoint::Checkpoint, utils::generate_default_validators};
    use ream_fork_choice_lean::genesis::setup_genesis;
    use ream_network_spec::networks::lean_network_spec;
    use ream_sync::rwlock::Writer;
    use ream_test_utils::store::sample_store;
    use url::Url;

    use super::get_justified_checkpoint;
    use crate::handlers::state::get_state;

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

    #[tokio::test]
    async fn test_client_fetches_and_deserializes_state() {
        let store = sample_store(10).await;
        let (_writer, reader) = Writer::new(store);

        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind address");
        let addr = listener.local_addr().expect("Failed to get local addr");

        let server = HttpServer::new(move || {
            App::new()
                .app_data(Data::new(reader.clone()))
                .service(scope("/lean/v0").service(get_state))
        })
        .listen(listener)
        .expect("Failed to attach listener")
        .run();

        let server_handle = server.handle();
        tokio::spawn(server);

        let client = LeanCheckpointClient::new();
        let base_url = Url::parse(&format!("http://{addr}")).expect("Failed to parse base URL");

        let state = client
            .fetch_finalized_state(&base_url)
            .await
            .expect("Client failed to fetch finalized state");

        assert_eq!(state.slot, 0);
        assert!(verify_checkpoint_state(&state));

        let (_, genesis_state) = setup_genesis(
            lean_network_spec().genesis_time,
            generate_default_validators(10),
        );

        assert_eq!(state, genesis_state);
        server_handle.stop(true).await;
    }
}
