use actix_web::web::ServiceConfig;

use crate::handlers::head::get_head;

/// Creates and returns all `/lean` routes.
pub fn register_lean_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_head);
}
