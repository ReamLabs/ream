use actix_web::web::ServiceConfig;

use crate::handlers::debug::{
    get_debug_beacon_heads, get_debug_beacon_state, get_debug_fork_choice,
};

pub fn register_debug_routes_v1(cfg: &mut ServiceConfig) {
    cfg.service(get_debug_fork_choice);
}

pub fn register_debug_routes_v2(cfg: &mut ServiceConfig) {
    cfg.service(get_debug_beacon_state)
        .service(get_debug_beacon_heads);
}
