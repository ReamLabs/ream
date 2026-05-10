use std::{io::Result, sync::Arc};

use actix_web::web::Data;
use ream_fork_choice_lean::store::{LeanStoreReader, LeanStoreWriter};
use ream_network_state_lean::NetworkState;
use ream_rpc_common::{config::RpcServerConfig, server::RpcServerBuilder};

use crate::{
    aggregator_controller::AggregatorController,
    routes::{register_routers, register_test_driver_routers},
};

/// Start the Lean API server.
pub async fn start(
    server_config: RpcServerConfig,
    lean_chain: LeanStoreReader,
    network_state: Arc<NetworkState>,
    aggregator_controller: Arc<AggregatorController>,
) -> Result<()> {
    RpcServerBuilder::new(server_config.http_socket_address)
        .allow_origin(server_config.http_allow_origin)
        .with_data(lean_chain)
        .with_data(network_state)
        .with_data(aggregator_controller)
        .configure(register_routers)
        .start()
        .await
}

/// Start the Lean API server in Hive test-driver mode.
pub async fn start_test_driver(
    server_config: RpcServerConfig,
    lean_chain: LeanStoreReader,
    lean_chain_writer: LeanStoreWriter,
    network_state: Arc<NetworkState>,
    aggregator_controller: Arc<AggregatorController>,
) -> Result<()> {
    RpcServerBuilder::new(server_config.http_socket_address)
        .allow_origin(server_config.http_allow_origin)
        .with_data(lean_chain)
        .with_app_data(Data::new(lean_chain_writer))
        .with_data(network_state)
        .with_data(aggregator_controller)
        .configure(register_test_driver_routers)
        .start()
        .await
}
