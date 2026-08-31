use actix_web::web::{PayloadConfig, ServiceConfig, scope};

use crate::handlers::{
    availability::get_availability,
    column::{get_column, get_columns},
    health::get_health,
    ingest::{MAX_BATCH_INGEST_BODY_BYTES, post_ingest, post_ingest_block},
    retention::post_retention,
};

/// Register every data-availability API route under the versioned `/data/v0` scope.
pub fn register_routers(config: &mut ServiceConfig) {
    config.service(
        scope("/data/v0")
            .app_data(PayloadConfig::new(MAX_BATCH_INGEST_BODY_BYTES))
            .configure(register_v0_routes),
    );
}

fn register_v0_routes(config: &mut ServiceConfig) {
    config.service(get_health);
    config.service(post_ingest);
    config.service(post_ingest_block);
    config.service(post_retention);
    config.service(get_availability);
    config.service(get_column);
    config.service(get_columns);
}
