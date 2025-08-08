pub mod lean;
use actix_web::web::{ServiceConfig, scope};

pub fn get_v0_routes(config: &mut ServiceConfig) {
    config.service(scope("/lean/v0").configure(lean::register_lean_routes));
}

pub fn register_routers(config: &mut ServiceConfig) {
    config.configure(get_v0_routes);
}
