use actix_web::web::{ServiceConfig, scope};

use crate::handlers::{
    availability::get_availability,
    column::{get_column, get_columns},
    health::get_health,
    ingest::post_ingest,
    retention::post_retention,
};

/// Register every DA API route.
///
/// Everything versioned lives under the `/da/v0` scope so the prefix can evolve
/// independently. Add new handlers (retention, availability, column serving) to
/// [`register_v0_routes`].
pub fn register_routers(config: &mut ServiceConfig) {
    config.service(scope("/da/v0").configure(register_v0_routes));
}

/// Routes served under the `/da/v0` scope.
fn register_v0_routes(config: &mut ServiceConfig) {
    config.service(get_health);
    config.service(post_ingest);
    config.service(post_retention);
    config.service(get_availability);
    config.service(get_column);
    config.service(get_columns);
}
