use actix_web::{HttpResponse, Responder, get, web};
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DebugResponse<T> {
    pub data: T,
}

#[get("/beacon/states")]
pub async fn get_debug_beacon_states(db: web::Data<ReamDB>) -> impl Responder {
    // TODO: Implement beacon states endpoint
    HttpResponse::Ok().json(DebugResponse {
        data: "beacon_states",
    })
}

#[get("/beacon/heads")]
pub async fn get_debug_beacon_heads(db: web::Data<ReamDB>) -> impl Responder {
    // TODO: Implement beacon heads endpoint
    HttpResponse::Ok().json(DebugResponse {
        data: "beacon_heads",
    })
}
