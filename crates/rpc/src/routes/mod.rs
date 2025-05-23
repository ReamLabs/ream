use actix_web::web::{ServiceConfig, scope};

pub mod beacon;
pub mod config;
pub mod debug;
pub mod node;

pub fn get_v1_routes(config: &mut ServiceConfig) {
    config.service(
        scope("/eth/v1")
            .configure(beacon::register_beacon_routes)
            .configure(debug::register_debug_routes)
            .configure(node::register_node_routes)
            .configure(config::register_config_routes),
    );
}

pub fn get_v2_routes(config: &mut ServiceConfig) {
    config.service(
        scope("/eth/v2")
            .configure(debug::register_debug_routes_v2)
            .configure(beacon::register_beacon_routes_v2),
    );
}

pub fn register_routers(config: &mut ServiceConfig) {
    config.configure(get_v1_routes).configure(get_v2_routes);
}
