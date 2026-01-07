use std::{io::Result, sync::Arc};

use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_events_beacon::BeaconEvent;
use ream_execution_engine::ExecutionEngine;
use ream_network_manager::p2p_sender::P2PSender;
use ream_operation_pool::OperationPool;
use ream_p2p::network::beacon::network_state::NetworkState;
use ream_rpc_common::{config::RpcServerConfig, server::RpcServerBuilder};
use ream_storage::{cache::BeaconCacheDB, db::beacon::BeaconDB};
use ream_sync_committee_pool::SyncCommitteePool;
use ream_validator_beacon::builder::builder_client::BuilderClient;
use tokio::sync::broadcast;

use crate::routes::register_routers;

/// Start the Beacon API server.
#[allow(clippy::too_many_arguments)]
pub async fn start(
    server_config: RpcServerConfig,
    db: BeaconDB,
    network_state: Arc<NetworkState>,
    operation_pool: Arc<OperationPool>,
    sync_committee_pool: Arc<SyncCommitteePool>,
    execution_engine: Option<ExecutionEngine>,
    builder_client: Option<Arc<BuilderClient>>,
    event_sender: broadcast::Sender<BeaconEvent>,
    beacon_chain: Arc<BeaconChain>,
    p2p_sender: Arc<P2PSender>,
    cached_db: Arc<BeaconCacheDB>,
) -> Result<()> {
    RpcServerBuilder::new(server_config.http_socket_address)
        .allow_origin(server_config.http_allow_origin)
        .with_data(db)
        .with_data(network_state)
        .with_data(operation_pool)
        .with_data(sync_committee_pool)
        .with_data(execution_engine)
        .with_data(builder_client)
        .with_data(event_sender)
        .with_data(beacon_chain)
        .with_data(p2p_sender)
        .with_data(cached_db)
        .configure(register_routers)
        .start()
        .await
}
