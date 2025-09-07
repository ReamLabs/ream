use actix_web::web::ServiceConfig;
use ream_rpc_common::handlers::version::get_version;

use crate::handlers::peer::{get_peer_count, list_peers};

/// Creates and returns all `/node` routes.
pub fn register_node_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_version)
        .service(get_peer_count)
        .service(list_peers);
}
