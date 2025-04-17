use actix_web::web;

use crate::handlers::config::{get_config_deposit_contract, get_config_spec};

pub fn register_config_routes(cfg: &mut web::ServiceConfig) {
    cfg
        // .app_data()// TODO: add network_spec
        .service(get_config_spec)
        .service(get_config_deposit_contract);
}
