use actix_web::{HttpResponse, Responder, get};
use serde::Serialize;

/// JSON body of `GET /data/v0/health`.
#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

/// `GET /data/v0/health` — liveness probe
#[get("/health")]
pub async fn get_health() -> impl Responder {
    HttpResponse::Ok().json(HealthResponse {
        status: "healthy",
        service: "data-availability-node",
    })
}
