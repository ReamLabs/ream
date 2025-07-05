use actix_web::web::ServiceConfig;

use crate::handlers::{
    identity::get_identity,
    peers::{get_peer, get_peer_count},
    syncing::get_syncing_status,
    version::get_version,
};

pub fn register_node_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_version)
        .service(get_peer)
        .service(get_peer_count)
        .service(get_syncing_status)
        .service(get_identity);
}
