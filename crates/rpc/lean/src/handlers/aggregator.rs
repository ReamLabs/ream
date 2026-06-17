use std::sync::Arc;

use actix_web::{HttpResponse, Responder, delete, get, post, web};
use ream_api_types_common::error::ApiError;
use ream_network_state_lean::AggregatorState;
use serde::{Deserialize, Serialize};
use tracing::info;

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
    aggregator_state: Option<web::Data<Arc<AggregatorState>>>,
) -> Result<impl Responder, ApiError> {
    let aggregator_state = aggregator_state
        .ok_or_else(|| ApiError::InternalError("Aggregator state not available".to_string()))?;

    Ok(HttpResponse::Ok().json(AggregatorStatus {
        is_aggregator: aggregator_state.is_enabled(),
    }))
}

/// POST /lean/v0/admin/aggregator
#[post("/admin/aggregator")]
pub async fn handle_toggle(
    aggregator_state: Option<web::Data<Arc<AggregatorState>>>,
    payload: web::Json<ToggleRequest>,
) -> Result<impl Responder, ApiError> {
    let aggregator_state = aggregator_state
        .ok_or_else(|| ApiError::InternalError("Aggregator state not available".to_string()))?;

    let enabled = payload.enabled;
    let previous = aggregator_state.set_enabled(enabled);

    if previous != enabled {
        info!(
            "Aggregator role {} via admin API; prover setup and subnet subscriptions remain boot-time configuration",
            if enabled { "activated" } else { "deactivated" }
        );
    }

    Ok(HttpResponse::Ok().json(ToggleResponse {
        is_aggregator: enabled,
        previous,
    }))
}

/// DELETE /lean/v0/admin/aggregator
#[delete("/admin/aggregator")]
pub async fn handle_delete() -> impl Responder {
    HttpResponse::MethodNotAllowed().finish()
}

#[cfg(test)]
mod tests {
    use actix_web::{App, test, web::Data};

    use super::*;

    fn setup_test_aggregator_state(initial_state: bool) -> Arc<AggregatorState> {
        Arc::new(AggregatorState::new(initial_state))
    }

    #[actix_web::test]
    async fn test_handle_status_happy_path() {
        let aggregator_state = setup_test_aggregator_state(false);
        let app = test::init_service(
            App::new()
                .app_data(Data::new(aggregator_state.clone()))
                .service(handle_status),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/admin/aggregator")
            .to_request();
        let response: AggregatorStatus = test::call_and_read_body_json(&app, request).await;

        assert!(!response.is_aggregator);
    }

    #[actix_web::test]
    async fn test_handle_toggle_updates_shared_state() {
        let aggregator_state = setup_test_aggregator_state(false);
        let app = test::init_service(
            App::new()
                .app_data(Data::new(aggregator_state.clone()))
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
        assert!(aggregator_state.is_enabled());

        let request = test::TestRequest::get()
            .uri("/admin/aggregator")
            .to_request();
        let response: AggregatorStatus = test::call_and_read_body_json(&app, request).await;
        assert!(response.is_aggregator);

        let request = test::TestRequest::post()
            .uri("/admin/aggregator")
            .set_json(ToggleRequest { enabled: false })
            .to_request();

        let response: ToggleResponse = test::call_and_read_body_json(&app, request).await;
        assert!(!response.is_aggregator);
        assert!(response.previous);
        assert!(!aggregator_state.is_enabled());

        let request = test::TestRequest::get()
            .uri("/admin/aggregator")
            .to_request();
        let response: AggregatorStatus = test::call_and_read_body_json(&app, request).await;
        assert!(!response.is_aggregator);
    }

    #[actix_web::test]
    async fn test_handle_toggle_returns_error_when_no_controller() {
        let app = test::init_service(App::new().service(handle_toggle)).await;

        let request = test::TestRequest::post()
            .uri("/admin/aggregator")
            .set_json(ToggleRequest { enabled: true })
            .to_request();

        let response = test::call_service(&app, request).await;
        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[actix_web::test]
    async fn test_handle_delete_returns_method_not_allowed() {
        let app = test::init_service(App::new().service(handle_delete)).await;

        let request = test::TestRequest::delete()
            .uri("/admin/aggregator")
            .to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::METHOD_NOT_ALLOWED
        );
    }
}
