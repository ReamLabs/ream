use actix_web::web::ServiceConfig;

use crate::handlers::{
    blob_sidecar::get_blob_sidecars,
    block::{
        get_block_attestations, get_block_from_id, get_block_rewards, get_block_root, get_genesis,
    },
    committee::get_committees,
    header::{get_headers, get_headers_from_block},
    state::{
        get_pending_consolidations, get_pending_deposits, get_pending_partial_withdrawals,
        get_state_finality_checkpoint, get_state_fork, get_state_randao, get_state_root,
        get_sync_committees,
    },
    validator::{
        get_validator_balances_from_state, get_validator_from_state, get_validators_from_state,
        post_validator_balances_from_state, post_validator_identities_from_state,
        post_validators_from_state, post_validator_liveness,
    },
};

/// Creates and returns all `/beacon` routes.
pub fn register_beacon_routes(cfg: &mut ServiceConfig) {
    cfg.service(get_blob_sidecars)
        .service(get_block_rewards)
        .service(get_block_root)
        .service(get_committees)
        .service(get_genesis)
        .service(get_headers)
        .service(get_headers_from_block)
        .service(get_pending_consolidations)
        .service(get_pending_deposits)
        .service(get_pending_partial_withdrawals)
        .service(get_sync_committees)
        .service(get_state_finality_checkpoint)
        .service(get_state_fork)
        .service(get_state_randao)
        .service(get_state_root)
        .service(get_validator_from_state)
        .service(get_validators_from_state)
        .service(post_validator_identities_from_state)
        .service(post_validators_from_state)
        .service(get_validator_balances_from_state)
        .service(post_validator_balances_from_state)
        .service(post_validator_liveness);
}

pub fn register_beacon_routes_v2(cfg: &mut ServiceConfig) {
    cfg.service(get_block_attestations)
        .service(get_block_from_id);
}
