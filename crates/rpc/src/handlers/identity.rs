use std::sync::Arc;

use ream_network_spec::identity::Identity;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::Data;

/// Called by `/identity` to get the Identity of the current running Node.
pub async fn get_node_identity(p2p_config: Arc<Identity>) -> Result<impl Reply, Rejection> {
    Ok(with_status(Data::json(p2p_config), StatusCode::OK))
}
