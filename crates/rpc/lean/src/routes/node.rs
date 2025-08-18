use actix_web::web::ServiceConfig;

use crate::handlers::peer::{get_peer_count, list_peers};

/// Creates and returns all `/node` routes.
pub fn register_node_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_peer_count).service(list_peers);
}
