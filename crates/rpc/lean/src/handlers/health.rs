use actix_web::{HttpResponse, Responder, get};

// GET /lean/v0/health
#[get("/health")]
pub async fn get_health() -> impl Responder {
    HttpResponse::Ok()
        .content_type("application/json")
        .json(serde_json::json!({
            "status": "healthy",
            "service": "lean-rpc-api"
        }))
}
