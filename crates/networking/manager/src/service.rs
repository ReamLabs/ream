use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_discv5::{
    config::DiscoveryConfig,
    subnet::{AttestationSubnets, SyncCommitteeSubnets},
};
use ream_execution_engine::ExecutionEngine;
use ream_executor::ReamExecutor;
use ream_network_spec::networks::network_spec;
use ream_operation_pool::OperationPool;
use ream_p2p::{
    config::NetworkConfig,
    network::{Network, ReamNetworkEvent},
    network_state::NetworkState,
};
use ream_storage::db::ReamDB;
use ream_syncer::block_range::BlockRangeSyncer;
use tokio::{sync::mpsc, time::interval};
use tracing::{error, info};

use crate::{
    config::ManagerConfig,
    gossipsub::{handle_gossipsub_message, init_gossipsub_config_with_topics},
    p2p_sender::P2PSender,
    req_resp::handle_req_resp_message,
};

pub struct NetworkManagerService {
    pub beacon_chain: Arc<BeaconChain>,
    manager_receiver: mpsc::UnboundedReceiver<ReamNetworkEvent>,
    p2p_sender: P2PSender,
    pub network_state: Arc<NetworkState>,
    pub block_range_syncer: BlockRangeSyncer,
    pub ream_db: ReamDB,
}

/// The `NetworkManagerService` acts as the manager for all networking activities in Ream.
/// Its core responsibilities include:
/// - Managing interactions between discovery, gossipsub, and sync protocols
/// - Routing messages from network protocols to the beacon chain logic
/// - Handling peer lifecycle management and connection state
impl NetworkManagerService {
    /// Creates a new `NetworkManagerService` instance.
    ///
    /// This function initializes the manager service by configuring:
    /// - discv5 configurations such as bootnodes, socket address, port, attestation subnets, sync
    ///   committee subnets, etc.
    /// - The gossipsub topics to subscribe to
    ///
    /// Upon successful configuration, it starts the network worker.
    pub async fn new(
        executor: ReamExecutor,
        config: ManagerConfig,
        ream_db: ReamDB,
        ream_dir: PathBuf,
        operation_pool: Arc<OperationPool>,
    ) -> anyhow::Result<Self> {
        let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
            config.socket_address,
            config.discovery_port,
        ))
        .build();

        let bootnodes = config.bootnodes.to_enrs(network_spec().network.clone());
        let discv5_config = DiscoveryConfig {
            discv5_config,
            bootnodes,
            socket_address: config.socket_address,
            socket_port: config.socket_port,
            discovery_port: config.discovery_port,
            disable_discovery: config.disable_discovery,
            attestation_subnets: AttestationSubnets::new(),
            sync_committee_subnets: SyncCommitteeSubnets::new(),
        };

        let gossipsub_config = init_gossipsub_config_with_topics();

        let network_config = NetworkConfig {
            discv5_config,
            gossipsub_config,
            data_dir: ream_dir,
        };

        let (manager_sender, manager_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, p2p_receiver) = mpsc::unbounded_channel();

        let execution_engine = if let (Some(execution_endpoint), Some(jwt_path)) =
            (config.execution_endpoint, config.execution_jwt_secret)
        {
            Some(ExecutionEngine::new(execution_endpoint, jwt_path)?)
        } else {
            None
        };
        let beacon_chain = Arc::new(BeaconChain::new(
            ream_db.clone(),
            operation_pool,
            execution_engine,
        ));
        let status = beacon_chain.build_status_request().await?;

        let network = Network::init(executor.clone(), &network_config, status).await?;

        let network_state = network.network_state();

        executor.spawn(async move {
            network.start(manager_sender, p2p_receiver).await;
        });

        let block_range_syncer = BlockRangeSyncer::new(
            beacon_chain.clone(),
            p2p_sender.clone(),
            network_state.clone(),
            executor.clone(),
        );

        Ok(Self {
            beacon_chain,
            manager_receiver,
            p2p_sender: P2PSender(p2p_sender),
            network_state,
            block_range_syncer,
            ream_db,
        })
    }

    /// Starts the manager service, which receives either a Gossipsub message or Req/Resp message
    /// from the network worker, and dispatches them to the appropriate handlers.
    ///
    /// Panics if the manager receiver is not initialized.
    pub async fn start(self) {
        let NetworkManagerService {
            beacon_chain,
            mut manager_receiver,
            p2p_sender,
            ream_db,
            network_state,
            block_range_syncer,
            ..
        } = self;

        let mut interval = interval(Duration::from_secs(network_spec().seconds_per_slot));
        let mut syncer_handle = block_range_syncer.start();
        loop {
            tokio::select! {
                result = &mut syncer_handle => {
                    let joined_result = match result {
                        Ok(joined_result) => joined_result,
                        Err(err) => {
                            error!("Block range syncer failed to join task: {err}");
                            continue;
                        }
                    };

                    let thread_result = match joined_result {
                        Ok(result) => result,
                        Err(err) => {
                            error!("Block range syncer thread failed: {err}");
                            continue;
                        }
                    };

                    let block_range_syncer = match thread_result {
                        Ok(syncer) => syncer,
                        Err(err) => {
                            error!("Block range syncer failed to start: {err}");
                            continue;
                        }
                    };

                    if !block_range_syncer.is_synced_to_finalized_slot().await {
                        syncer_handle = block_range_syncer.start();
                    }
                }
                _ = interval.tick() => {
                    let time = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("correct time")
                        .as_secs();

                    if let Err(err) =  beacon_chain.process_tick(time).await {
                        error!("Failed to process gossipsub tick: {err}");
                    }
                }
                Some(event) = manager_receiver.recv() => {
                    match event {
                        // Handles Gossipsub messages from other peers.
                        ReamNetworkEvent::GossipsubMessage { message } =>
                            handle_gossipsub_message(message, &beacon_chain).await,
                        // Handles Req/Resp messages from other peers.
                        ReamNetworkEvent::RequestMessage { peer_id, stream_id, connection_id, message } =>
                            handle_req_resp_message(peer_id, stream_id, connection_id, message, &p2p_sender, &ream_db, network_state.clone()).await,
                        // Log and skip unrecognized requests.
                        unhandled_request => {
                            info!("Unhandled request: {unhandled_request:?}");
                        }
                    }
                }
            }
        }
    }
}
