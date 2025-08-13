use actix_web::{HttpResponse, Responder, get};
use ream_api_types_beacon::{error::ApiError, responses::DataResponse};
use ream_node::version::ream_node_version;
use serde::{Deserialize, Serialize};

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
pub async fn get_version() -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataResponse::new(Version::new())))
}
