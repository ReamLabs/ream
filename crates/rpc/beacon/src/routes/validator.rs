use actix_web::web::ServiceConfig;

use crate::handlers::{
    duties::{get_attester_duties, get_proposer_duties},
    prepare_beacon_proposer::prepare_beacon_proposer,
    validator::{get_attestation_data, post_beacon_committee_selections},
};

pub fn register_validator_routes(config: &mut ServiceConfig) {
    config.service(get_proposer_duties);
    config.service(get_attester_duties);
    config.service(prepare_beacon_proposer);
    config.service(get_attestation_data);
    config.service(post_beacon_committee_selections);
}
