use actix_web::web;

use crate::handlers::state::get_beacon_state;

pub fn register_debug_routes_v2(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/debug").service(get_beacon_state));
}
