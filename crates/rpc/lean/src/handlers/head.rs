use std::sync::Arc;

use actix_web::{HttpResponse, Responder, get, web::Data};
use ream_beacon_api_types::error::ApiError;
use ream_chain_lean::lean_chain::LeanChain;
use ream_lean_api_types::head::Head;
use tokio::sync::RwLock;

// GET /lean/v0/head
#[get("/head")]
pub async fn get_head(
    lean_chain: Data<Arc<RwLock<LeanChain>>>,
) -> Result<impl Responder, ApiError> {
    println!("got request");
    Ok(HttpResponse::Ok().json(Head {
        head: lean_chain.read().await.head,
    }))
}
