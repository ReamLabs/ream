use actix_web::web::ServiceConfig;

use crate::handlers::{
    duties::{get_attester_duties, get_proposer_duties, get_sync_committee_duties},
    prepare_beacon_proposer::prepare_beacon_proposer,
    validator::{
        get_aggregate_attestation, get_attestation_data, get_blocks_v3,
        get_sync_committee_contribution, post_aggregate_and_proofs_v2,
        post_beacon_committee_selections, post_beacon_committee_subscriptions,
        post_contribution_and_proofs, post_register_validator, post_sync_committee_subscriptions,
    },
};

pub fn register_validator_routes_v1(config: &mut ServiceConfig) {
    config.service(get_proposer_duties);
    config.service(get_attester_duties);
    config.service(get_sync_committee_duties);
    config.service(prepare_beacon_proposer);
    config.service(get_attestation_data);
    config.service(post_beacon_committee_selections);
    config.service(get_sync_committee_contribution);
    config.service(post_beacon_committee_subscriptions);
    config.service(post_sync_committee_subscriptions);
    config.service(post_contribution_and_proofs);
    config.service(post_register_validator);
}

pub fn register_validator_routes_v2(config: &mut ServiceConfig) {
    config.service(get_aggregate_attestation);
    config.service(post_aggregate_and_proofs_v2);
}

pub fn register_validator_routes_v3(config: &mut ServiceConfig) {
    config.service(get_blocks_v3);
}
