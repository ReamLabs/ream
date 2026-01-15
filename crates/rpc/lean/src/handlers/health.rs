use actix_web::{HttpResponse, Responder, get};

#[get("/health")]
pub async fn get_health() -> impl Responder {
    HttpResponse::Ok().body("healthy")
}
