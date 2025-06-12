use actix_web::web::ServiceConfig;

use crate::handlers::{
    peers::{get_peer, get_peer_count},
    version::get_version,
};

pub fn register_node_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_version)
        .service(get_peer)
        .service(get_peer_count);
}
