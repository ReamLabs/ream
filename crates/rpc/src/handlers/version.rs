use actix_web::{HttpResponse, Responder, get};
use ream_node::version::ream_node_version;
use serde::{Deserialize, Serialize};

use crate::types::response::DataResponse;

#[derive(Serialize, Deserialize, Default)]
pub struct Version {
    version: String,
}

impl Version {
    pub fn new() -> Self {
        Self {
            version: ream_node_version(),
        }
    }
}

/// Called by `eth/v1/node/version` to get the Node Version.
#[get("/node/version")]
pub async fn get_version() -> actix_web::Result<impl Responder> {
    Ok(HttpResponse::Ok().json(DataResponse::new(Version::new())))
}
