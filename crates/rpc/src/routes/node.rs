use actix_web::web::ServiceConfig;

use crate::handlers::{peers::get_peer, syncing::get_syncing_status, version::get_version};

pub fn register_node_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_version)
        .service(get_peer)
        .service(get_syncing_status);
}
