use actix_web::web::ServiceConfig;

use crate::handlers::{
    aggregator::{handle_status, handle_toggle}, block::get_block, block_header::get_block_header, checkpoint::get_justified_checkpoint, fork_choice::get_fork_choice_tree, head::get_head, health::get_health, state::get_state
};

/// Creates and returns all `/lean` routes.
pub fn register_lean_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_head)
        .service(get_block)
        .service(get_block_header)
        .service(get_fork_choice_tree)
        .service(get_justified_checkpoint)
        .service(get_state)
        .service(get_health)
        .service(handle_status)
        .service(handle_toggle);
}
