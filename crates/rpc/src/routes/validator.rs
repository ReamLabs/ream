use actix_web::web::ServiceConfig;

use crate::handlers::duties::get_proposer_duties;

pub fn register_validator_routes(config: &mut ServiceConfig) {
    config.service(get_proposer_duties);
}
