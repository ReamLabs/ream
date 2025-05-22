use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::electra::beacon_state::BeaconState;
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
use ream_storage::{db::ReamDB, tables::Table};
use ream_syncer::block_range::BlockRangeSyncer;
use tokio::{
    sync::{RwLock, mpsc},
    time::interval,
};
use tracing::{error, info, trace};

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
    pub sync_committee_subscriptions: Arc<RwLock<HashMap<u8, u64>>>,
    pub sync_committee_subnets: Arc<RwLock<SyncCommitteeSubnets>>,
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
            socket_address: config.socket_address,
            socket_port: config.socket_port,
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

        let block_range_syncer = BlockRangeSyncer::new(beacon_chain.clone(), p2p_sender.clone());

        let sync_committee_subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let sync_committee_subnets = Arc::new(RwLock::new(SyncCommitteeSubnets::new()));

        Ok(Self {
            beacon_chain,
            manager_receiver,
            p2p_sender: P2PSender(p2p_sender),
            network_state,
            block_range_syncer,
            ream_db,
            sync_committee_subscriptions,
            sync_committee_subnets,
        })
    }

    /// Fetch the latest BeaconState from the DB by highest slot.
    fn get_latest_beacon_state(db: &ReamDB) -> anyhow::Result<Option<BeaconState>> {
        let highest_slot = db
            .slot_index_provider()
            .get_highest_slot()?
            .ok_or_else(|| anyhow!("No highest slot found in database"))?;

        let block_root = db
            .slot_index_provider()
            .get(highest_slot)?
            .ok_or_else(|| anyhow!("No block root found for slot {highest_slot}"))?;
        let beacon_state = db.beacon_state_provider().get(block_root)?;
        Ok(beacon_state)
    }

    pub async fn check_and_expire_sync_committee_subscriptions(&self) {
        let state = match Self::get_latest_beacon_state(&self.ream_db) {
            Ok(Some(state)) => state,
            Ok(None) => {
                trace!("No beacon state available for sync committee subscription check");
                return;
            }
            Err(err) => {
                error!("Failed to get latest beacon state: {}", err);
                return;
            }
        };
        let current_epoch = state.get_current_epoch();
        let mut sync_committee_subscriptions = self.sync_committee_subscriptions.write().await;
        let expired_subnet_ids: Vec<u8> = sync_committee_subscriptions
            .iter()
            .filter(|&(&_subnet_id, &until_epoch)| until_epoch <= current_epoch)
            .map(|(&subnet_id, &_until_epoch)| subnet_id)
            .collect();
        if !expired_subnet_ids.is_empty() {
            let mut subnets = self.sync_committee_subnets.write().await;
            for subnet_id in &expired_subnet_ids {
                if let Err(err) = subnets.disable_sync_committee_subnet(*subnet_id) {
                    error!("Failed to disable sync committee subnet {subnet_id}: {err}",);
                }
                sync_committee_subscriptions.remove(subnet_id);
            }
            if !expired_subnet_ids.is_empty() {
                info!("Marked that ENR needs to be updated after sync committee subnet expiry");
            }
        }
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
            sync_committee_subscriptions,
            sync_committee_subnets,
            ..
        } = self;
        let network_spec = network_spec();
        let mut slot_interval = interval(Duration::from_secs(network_spec.seconds_per_slot));
        let mut expiry_interval = interval(Duration::from_secs(12 * 32));
        loop {
            tokio::select! {
                _ = slot_interval.tick() => {
                    let time = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("correct time")
                        .as_secs();

                    if let Err(err) = beacon_chain.process_tick(time).await {
                        error!("Failed to process gossipsub tick: {err}");
                    }
                }
                _ = expiry_interval.tick() => {
                    // Check and expire sync committee subscriptions
                    let state = match Self::get_latest_beacon_state(&ream_db) {
                        Ok(Some(state)) => state,
                        Ok(None) => {
                            trace!("No beacon state available for sync committee subscription check");
                            continue;
                        }
                        Err(err) => {
                            error!("Failed to get latest beacon state: {}", err);
                            continue;
                        }
                    };
                    let current_epoch = state.get_current_epoch();
                    let mut sync_committee_subscriptions = sync_committee_subscriptions.write().await;
                    let expired_subnet_ids: Vec<u8> = sync_committee_subscriptions
                        .iter()
                        .filter(|&(&_subnet_id, &until_epoch)| until_epoch <= current_epoch)
                        .map(|(&subnet_id, &_until_epoch)| subnet_id)
                        .collect();
                    if !expired_subnet_ids.is_empty() {
                        let mut subnets = sync_committee_subnets.write().await;
                        for subnet_id in &expired_subnet_ids {
                            if let Err(err) = subnets.disable_sync_committee_subnet(*subnet_id) {
                                error!("Failed to disable sync committee subnet {subnet_id}: {err}",);
                            }
                            sync_committee_subscriptions.remove(subnet_id);
                        }
                        if !expired_subnet_ids.is_empty() {
                            info!("Marked that ENR needs to be updated after sync committee subnet expiry");
                        }
                    }
                }
                Some(event) = manager_receiver.recv() => {
                    match event {
                        // Handles Gossipsub messages from other peers.
                        ReamNetworkEvent::GossipsubMessage { message } =>
                            handle_gossipsub_message(message, &beacon_chain).await,
                        // Handles Req/Resp messages from other peers.
                        ReamNetworkEvent::RequestMessage { peer_id, stream_id, connection_id, message } =>
                            handle_req_resp_message(peer_id, stream_id, connection_id, message, &beacon_chain, &p2p_sender, &ream_db).await,
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
