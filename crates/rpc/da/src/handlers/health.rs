use actix_web::{HttpResponse, Responder, get};
use serde::Serialize;

/// JSON body of `GET /da/v0/health`.
#[derive(Serialize)]
pub struct HealthResponse {
    /// Liveness status; `"healthy"` whenever the process can answer at all.
    status: &'static str,
    /// Service name, so a probe hitting several local nodes can tell them apart.
    service: &'static str,
}

/// `GET /da/v0/health` — liveness probe.
///
/// Returns 200 as long as the HTTP server is up. It deliberately touches neither
/// the store nor the verifier: it reports that the process is reachable, not that
/// the DA dataset is in any particular state — use `GET /availability/{block_root}`
/// for that.
#[get("/health")]
pub async fn get_health() -> impl Responder {
    HttpResponse::Ok().json(HealthResponse {
        status: "healthy",
        service: "da-node",
    })
}
