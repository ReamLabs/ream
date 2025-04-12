use ream_node::network_channel::NetworkChannel;
use warp::{Filter, Rejection, filters::path::end, get, log, path, reply::Reply};

use crate::handlers::{peer_count::get_peer_count, version::get_version};

/// Creates and returns all `/node` routes.
pub fn get_node_routes(
    network_channel: NetworkChannel,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let version = path("node")
        .and(path("version"))
        .and(end())
        .and(get())
        .and_then(get_version)
        .with(log("version"));

    let peer_count = path("node")
        .and(path("peer_count"))
        .and(end())
        .and(get())
        .and(warp::any().map(move || network_channel.clone()))
        .and_then(get_peer_count)
        .with(log("peer_count"));

    version.or(peer_count)
}
