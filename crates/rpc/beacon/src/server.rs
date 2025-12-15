use std::{io::Result, sync::Arc};

use ream_events_beacon::BeaconEvent;
use ream_execution_engine::ExecutionEngine;
use ream_operation_pool::OperationPool;
use ream_p2p::network::beacon::network_state::NetworkState;
use ream_rpc_common::{config::RpcServerConfig, server::RpcServerBuilder};
use ream_storage::db::beacon::BeaconDB;
use ream_validator_beacon::builder::builder_client::BuilderClient;
use tokio::sync::broadcast;

use crate::routes::register_routers;

/// Start the Beacon API server.
pub async fn start(
    server_config: RpcServerConfig,
    db: BeaconDB,
    network_state: Arc<NetworkState>,
    operation_pool: Arc<OperationPool>,
    execution_engine: Option<ExecutionEngine>,
    builder_client: Option<Arc<BuilderClient>>,
    event_sender: broadcast::Sender<BeaconEvent>,
) -> Result<()> {
    RpcServerBuilder::new(server_config.http_socket_address)
        .allow_origin(server_config.http_allow_origin)
        .with_data(db)
        .with_data(network_state)
        .with_data(operation_pool)
        .with_data(execution_engine)
        .with_data(builder_client)
        .with_data(event_sender)
        .configure(register_routers)
        .start()
        .await
}
