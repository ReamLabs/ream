use actix_web::web::ServiceConfig;

use crate::handlers::config::{get_config_deposit_contract, get_config_spec, get_fork_schedule};

pub fn register_config_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_config_spec)
        .service(get_config_deposit_contract)
        .service(get_fork_schedule);
}
