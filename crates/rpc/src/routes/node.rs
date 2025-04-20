use actix_web::web::ServiceConfig;

use crate::handlers::version::get_version;

pub fn register_node_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_version);
}
