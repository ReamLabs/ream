use actix_web::{HttpResponse, Responder, get};
use ream_node::version::ream_node_version;
use serde::{Deserialize, Serialize};
// use warp::{
//     http::status::StatusCode,
//     reject::Rejection,
//     reply::{Reply, with_status},
// };

// use super::Data;

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
#[get("node/version")]
pub async fn get_version() -> impl Responder {
    HttpResponse::Ok().json(Version::new())
}
