use std::sync::Arc;

use actix_web::{App, HttpServer, middleware, web::Data};
use config::RpcServerConfig;
use ream_execution_engine::ExecutionEngine;
use ream_operation_pool::OperationPool;
use ream_p2p::network::beacon::network_state::NetworkState;
use ream_storage::db::ReamDB;
use tracing::info;

use crate::routes::register_routers;

pub mod config;
pub mod handlers;
pub mod routes;

/// Start the Beacon API server.
pub async fn start_server(
    server_config: RpcServerConfig,
    db: ReamDB,
    network_state: Arc<NetworkState>,
    operation_pool: Arc<OperationPool>,
    execution_engine: Option<ExecutionEngine>,
) -> std::io::Result<()> {
    info!(
        "starting HTTP server on {:?}",
        server_config.http_socket_address
    );

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .app_data(Data::new(db.clone()))
            .app_data(Data::new(network_state.clone()))
            .app_data(Data::new(operation_pool.clone()))
            .app_data(Data::new(execution_engine.clone()))
            .configure(register_routers)
    })
    .bind(server_config.http_socket_address)?
    .run();

    server.await
}
