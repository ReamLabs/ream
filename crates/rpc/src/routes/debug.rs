use actix_web::web::ServiceConfig;

use crate::handlers::debug::{get_beacon_heads, get_beacon_state, get_fork_choice};

pub fn register_debug_routes_v1(cfg: &mut ServiceConfig) {
    cfg.service(get_fork_choice);
}

pub fn register_debug_routes_v2(cfg: &mut ServiceConfig) {
    cfg.service(get_beacon_state).service(get_beacon_heads);
}
