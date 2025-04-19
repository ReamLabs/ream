use ream_storage::db::ReamDB;
use warp::{
    Filter, Rejection,
    filters::path::{end, param},
    get, log, path,
    reply::Reply,
};

use super::with_db;
use crate::{handlers::{block::get_beacon_heads, state::get_state}, types::id::ID};

/// Creates and returns all `/debug` routes.
pub fn get_debug_routes_v2(
    db: ReamDB,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let db_filter = with_db(db);

    let debug_base = path("debug");
    
    let beacon_state = debug_base
        .and(path("beacon"))
        .and(path("states"))
        .and(param::<ID>())
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(move |state_id: ID, db: ReamDB| get_state(state_id, db))
        .with(log("beacon_state"));

    let beacon_block_heads = debug_base
        .and(path("beacon"))
        .and(path("heads"))
        .and(end())
        .and(get())
        .and(db_filter.clone())
        .and_then(get_beacon_heads)
        .with(log("beacon_block_heads"));

    beacon_state.or(beacon_block_heads)
}
