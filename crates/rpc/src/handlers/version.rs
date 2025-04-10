use ream_node::version::ream_node_version;
use serde::{Deserialize, Serialize};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::Data;

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

/// Called by `/version` to get the Node Version.
pub async fn get_version() -> Result<impl Reply, Rejection> {
    Ok(with_status(Data::json(Version::new()), StatusCode::OK))
}
