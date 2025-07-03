use actix_web::web::{ServiceConfig, scope};

use crate::handlers::{block::get_beacon_heads, state::get_beacon_state};

pub fn register_debug_routes_v2(cfg: &mut ServiceConfig) {
    cfg.service(
        scope("/debug")
            .service(get_beacon_state)
            .service(get_beacon_heads),
    );
}
