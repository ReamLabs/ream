use actix_web::web;

use crate::handlers::version::get_version;

pub fn register_node_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_version);
}
