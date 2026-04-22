use actix_web::{get, post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use ream_api_types_common::error::ApiError;
use std::sync::Arc;

use crate::aggregator_controller::AggregatorController;

#[derive(Serialize, Deserialize)]
pub struct AggregatorStatus {
    pub is_aggregator: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ToggleRequest {
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ToggleResponse {
    pub is_aggregator: bool,
    pub previous: bool,
}

/// GET /lean/v0/admin/aggregator
#[get("/admin/aggregator")]
pub async fn handle_status(
    controller: Option<web::Data<Arc<AggregatorController>>>,
) -> Result<impl Responder, ApiError> {
    let controller = controller.ok_or_else(|| {
        ApiError::InternalError("Aggregator controller not available".to_string())
    })?;

    Ok(HttpResponse::Ok().json(AggregatorStatus {
        is_aggregator: controller.is_enabled(),
    }))
}

/// POST /lean/v0/admin/aggregator
#[post("/admin/aggregator")]
pub async fn handle_toggle(
    controller: Option<web::Data<Arc<AggregatorController>>>,
    payload: web::Json<ToggleRequest>,
) -> Result<impl Responder, ApiError> {
    let controller = controller.ok_or_else(|| {
        ApiError::InternalError("Aggregator controller not available".to_string())
    })?;

    let enabled = payload.enabled;
    let previous = controller.set_enabled(enabled);

    Ok(HttpResponse::Ok().json(ToggleResponse {
        is_aggregator: enabled,
        previous,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App, web::Data};
    use ream_network_state_lean::NetworkState;
    use ream_consensus_lean::checkpoint::Checkpoint;

    fn setup_test_controller(initial_state: bool) -> Arc<AggregatorController> {
        let network_state = Arc::new(NetworkState::new(
            Checkpoint::default(),
            Checkpoint::default(),
            initial_state,
        ));
        Arc::new(AggregatorController::new(network_state))
    }

    #[actix_web::test]
    async fn test_handle_status_happy_path() {
        let controller = setup_test_controller(false);
        let app = test::init_service(
            App::new()
                .app_data(Data::new(controller.clone()))
                .service(handle_status),
        )
        .await;

        let request = test::TestRequest::get().uri("/admin/aggregator").to_request();
        let response: AggregatorStatus = test::call_and_read_body_json(&app, request).await;

        assert!(!response.is_aggregator);
    }

    #[actix_web::test]
    async fn test_handle_toggle_updates_state() {
        let controller = setup_test_controller(false);
        let app = test::init_service(
            App::new()
                .app_data(Data::new(controller.clone()))
                .service(handle_toggle)
                .service(handle_status),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/admin/aggregator")
            .set_json(ToggleRequest { enabled: true })
            .to_request();
        
        let response: ToggleResponse = test::call_and_read_body_json(&app, request).await;
        assert!(response.is_aggregator);
        assert!(!response.previous);

        let request = test::TestRequest::get().uri("/admin/aggregator").to_request();
        let response: AggregatorStatus = test::call_and_read_body_json(&app, request).await;
        assert!(response.is_aggregator);
    }

    #[actix_web::test]
    async fn test_handle_toggle_returns_error_when_no_controller() {
        let app = test::init_service(
            App::new().service(handle_toggle)
        ).await;

        let request = test::TestRequest::post()
            .uri("/admin/aggregator")
            .set_json(ToggleRequest { enabled: true })
            .to_request();

        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
}
