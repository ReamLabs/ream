pub mod config;
pub mod handlers;
pub mod routes;

use std::sync::Arc;

use actix_web::web::Data;
use config::RpcServerConfig;
use ream_execution_engine::ExecutionEngine;
use ream_operation_pool::OperationPool;
use ream_p2p::network::beacon::network_state::NetworkState;
use ream_rpc_common::server::start_rpc_server;
use ream_storage::db::beacon::BeaconDB;

use crate::routes::register_routers;

/// Start the Beacon API server.
pub async fn start_server(
    server_config: RpcServerConfig,
    db: BeaconDB,
    network_state: Arc<NetworkState>,
    operation_pool: Arc<OperationPool>,
    execution_engine: Option<ExecutionEngine>,
) -> std::io::Result<()> {
    let server = start_rpc_server(server_config.http_socket_address, move |cfg| {
        cfg.app_data(Data::new(db.clone()))
            .app_data(Data::new(network_state.clone()))
            .app_data(Data::new(operation_pool.clone()))
            .app_data(Data::new(execution_engine.clone()))
            .configure(register_routers);
    })?;

    server.await
}
