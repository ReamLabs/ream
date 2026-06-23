pub mod lean;
pub mod node;
use actix_web::web::{ServiceConfig, scope};

use crate::handlers::{health::get_health, metrics::get_metrics};

pub fn get_v0_routes(config: &mut ServiceConfig) {
    config.service(
        scope("/lean/v0")
            .configure(lean::register_lean_routes)
            .configure(node::register_node_routes),
    );
}

pub fn get_test_driver_v0_routes(config: &mut ServiceConfig) {
    config.service(
        scope("/lean/v0")
            .configure(lean::register_test_driver_lean_routes)
            .configure(node::register_node_routes),
    );
}

pub fn register_routers(config: &mut ServiceConfig) {
    config
        .configure(get_v0_routes)
        .service(get_health)
        .service(get_metrics);
}

pub fn register_test_driver_routers(config: &mut ServiceConfig) {
    config
        .configure(get_test_driver_v0_routes)
        .service(get_health)
        .service(get_metrics);
}
