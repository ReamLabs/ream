use actix_web::web::ServiceConfig;

use crate::handlers::{
    aggregator::{handle_status, handle_toggle},
    block::get_block,
    block_header::get_block_header,
    checkpoint::get_justified_checkpoint,
    fork_choice::get_fork_choice_tree,
    head::get_head,
    health::get_health,
    state::get_state,
    test_driver::{
        init_fork_choice, reset_store, run_state_transition, run_verify_signatures,
        snapshot_fork_choice, step_fork_choice,
    },
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

/// Creates and returns all `/lean` routes for the Hive test-driver mode.
pub fn register_test_driver_lean_routes(cfg: &mut ServiceConfig) {
    register_lean_routes(cfg);

    cfg.service(reset_store)
        .service(init_fork_choice)
        .service(step_fork_choice)
        .service(snapshot_fork_choice)
        .service(run_state_transition)
        .service(run_verify_signatures);
}
