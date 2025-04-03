use std::sync::Arc;

use ream_network_spec::identity::Identity;
use warp::{Filter, Rejection, filters::path::end, get, log, path, reply::Reply};

use crate::handlers::{identity::get_node_identity, version::get_version};

/// Creates and returns all `/node` routes.
pub fn get_node_routes(
    p2p_config: Arc<Identity>,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let base_path = path("node");

    let version_endpoint = base_path
        .and(path("version"))
        .and(end())
        .and(get())
        .and_then(get_version)
        .with(log("version"));

    let identity_endpoint = base_path
        .and(path("identity"))
        .and(end())
        .and(get())
        .and_then(move || get_node_identity(p2p_config.clone()))
        .with(log("identity"));

    version_endpoint.or(identity_endpoint)
}
