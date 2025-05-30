use actix_web::web::ServiceConfig;

use crate::handlers::duties::{get_attester_duties, get_proposer_duties};

pub fn register_validator_routes(config: &mut ServiceConfig) {
    config.service(get_proposer_duties);
    config.service(get_attester_duties);
}
