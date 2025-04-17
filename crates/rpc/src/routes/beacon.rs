use actix_web::web;

use crate::handlers::{
    block::{
        get_block_attestations, get_block_from_id, get_block_rewards, get_block_root, get_genesis,
    },
    header::get_headers,
    state::{
        get_state_finality_checkpoint, get_state_fork, get_state_randao, get_state_root,
        get_state_validator,
    },
};

pub fn register_beacon_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_state_root)
        .service(get_state_fork)
        .service(get_state_finality_checkpoint)
        .service(get_state_randao)
        .service(get_state_validator)
        .service(get_genesis)
        .service(get_headers)
        .service(get_block_root)
        .service(get_block_rewards);
}
pub fn register_beacon_routes_v2(cfg: &mut web::ServiceConfig) {
    cfg.service(get_block_attestations)
        .service(get_block_from_id);
}
