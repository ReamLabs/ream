use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get,
    http::StatusCode,
    web::{Data, Query},
};
use ream_api_types_beacon::query::HealthQuery;
use ream_api_types_common::error::ApiError;
use ream_execution_engine::ExecutionEngine;
use ream_operation_pool::OperationPool;
use ream_storage::db::beacon::BeaconDB;

use super::syncing::calculate_sync_status;

/// Called by `eth/v1/node/health` to check the node's health status.
///
/// Query parameters:
/// - `syncing_status`: Optional custom HTTP status code to use when syncing (instead of 206)
#[get("/node/health")]
pub async fn get_health(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    execution_engine: Data<Option<ExecutionEngine>>,
    query: Query<HealthQuery>,
) -> Result<impl Responder, ApiError> {
    // Validate custom syncing_status if provided
    if let Some(custom_code) = query.syncing_status
        && !(100..=599).contains(&custom_code)
    {
        return Err(ApiError::BadRequest(format!(
            "Invalid syncing status code: {custom_code}"
        )));
    }

    let sync_status = calculate_sync_status(&db, &operation_pool, &execution_engine).await?;

    if sync_status.is_syncing || sync_status.is_optimistic || sync_status.el_offline {
        let status_code: StatusCode = query
            .syncing_status
            .and_then(|code| StatusCode::from_u16(code).ok())
            .unwrap_or(StatusCode::PARTIAL_CONTENT); // Default 206

        Ok(HttpResponse::build(status_code).finish())
    } else {
        Ok(HttpResponse::build(StatusCode::OK).finish())
    }
}
