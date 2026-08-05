use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_misc::misc::compute_start_slot_at_epoch;
use ream_discv5::{
    config::DiscoveryConfig,
    subnet::{AttestationSubnets, CustodyGroupCount, SyncCommitteeSubnets},
};
use ream_executor::ReamExecutor;
use ream_fork_choice_beacon::data_availability::AvailabilityEntryStatus;
use ream_network_spec::networks::beacon_network_spec;
use ream_p2p::{
    config::NetworkConfig,
    network::beacon::{Network, ReamNetworkEvent, network_state::NetworkState},
};
use ream_storage::{
    cache::BeaconCacheDB,
    db::beacon::BeaconDB,
    tables::{field::REDBField, table::REDBTable},
};
use ream_sync_committee_pool::SyncCommitteePool;
use ream_syncer::block_range::BlockRangeSyncer;
use tokio::{sync::mpsc, time::interval};
use tracing::{error, info, warn};
use tree_hash::TreeHash;

use crate::{
    block_lookup::{
        BlockLookupConfig, BlockLookupCoordinator, PendingGossipItem, apply_block_import_event,
        apply_coordinator_update, insert_pending_item, log_insert_outcome,
        spawn_block_lookup_worker,
    },
    config::ManagerConfig,
    gossipsub::handle::{handle_gossipsub_message, init_gossipsub_config_with_topics},
    p2p_sender::P2PSender,
    req_resp::handle_req_resp_message,
};

pub struct NetworkManagerService {
    pub beacon_chain: Arc<BeaconChain>,
    manager_receiver: mpsc::UnboundedReceiver<ReamNetworkEvent>,
    pub p2p_sender: P2PSender,
    pub network_state: Arc<NetworkState>,
    pub block_range_syncer: BlockRangeSyncer,
    pub ream_db: BeaconDB,
    pub cached_db: Arc<BeaconCacheDB>,
    pub sync_committee_pool: Arc<SyncCommitteePool>,
}

struct ReconciledBlockLookupState {
    imported_roots: Vec<alloy_primitives::B256>,
    pending_availability_roots: Vec<alloy_primitives::B256>,
}

async fn reconcile_block_lookup_state(
    beacon_chain: &BeaconChain,
    roots: Vec<alloy_primitives::B256>,
) -> ReconciledBlockLookupState {
    let store = beacon_chain.store.lock().await;
    let mut imported_roots = roots.clone();
    imported_roots.retain(|block_root| {
        let has_block = store
            .db
            .block_provider()
            .get(*block_root)
            .is_ok_and(|block| block.is_some());
        let has_state = store
            .db
            .state_provider()
            .get(*block_root)
            .is_ok_and(|state| state.is_some());
        has_block && has_state
    });
    let pending_availability_roots = roots
        .into_iter()
        .filter(|block_root| {
            matches!(
                store.data_availability_checker.status(block_root),
                AvailabilityEntryStatus::PendingBlock | AvailabilityEntryStatus::Complete
            )
        })
        .collect();
    ReconciledBlockLookupState {
        imported_roots,
        pending_availability_roots,
    }
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
        ream_db: BeaconDB,
        ream_directory: PathBuf,
        beacon_chain: Arc<BeaconChain>,
        sync_committee_pool: Arc<SyncCommitteePool>,
        cached_db: Arc<BeaconCacheDB>,
    ) -> anyhow::Result<Self> {
        // Initialize the KZG trusted setup before validating data column sidecars to avoid delaying
        // the first gossipsub validation decision.
        executor
            .spawn_blocking(|| {
                ream_polynomial_commitments::trusted_setup::blst_settings();
            })
            .await?;

        let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
            config.socket_address,
            config.discovery_port,
        ))
        .build();

        let bootnodes = config
            .bootnodes
            .to_enrs_beacon(beacon_network_spec().network.clone());
        let discv5_config = DiscoveryConfig {
            discv5_config,
            bootnodes,
            socket_address: config.socket_address,
            socket_port: config.socket_port,
            discovery_port: config.discovery_port,
            disable_discovery: config.disable_discovery,
            attestation_subnets: AttestationSubnets::new(),
            sync_committee_subnets: SyncCommitteeSubnets::new(),
            custody_group_count: CustodyGroupCount::default(),
        };

        let gossipsub_config = init_gossipsub_config_with_topics(config.gossipsub_history_length);

        let network_config = NetworkConfig {
            discv5_config,
            gossipsub_config,
            data_dir: ream_directory,
        };

        let (manager_sender, manager_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, p2p_receiver) = mpsc::unbounded_channel();

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
            cached_db,
            sync_committee_pool,
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
            cached_db,
            network_state,
            block_range_syncer,
            ..
        } = self;

        let mut interval = interval(Duration::from_secs(
            beacon_network_spec().seconds_per_slot(),
        ));
        let mut block_import_receiver = beacon_chain.subscribe_block_imports();
        let mut block_import_receiver_active = true;
        let mut block_lookup_coordinator =
            BlockLookupCoordinator::new(BlockLookupConfig::for_data_column_retention(
                beacon_network_spec().min_epochs_for_data_column_sidecars_requests,
            ));
        let (block_lookup_action_sender, mut block_lookup_update_receiver) =
            spawn_block_lookup_worker(beacon_chain.clone());
        let mut block_lookup_worker_active = true;
        let mut syncer_handle = block_range_syncer.start();
        // Avoid polling a completed JoinHandle after the syncer has caught up.
        let mut syncer_active = true;
        loop {
            tokio::select! {
                permit = block_lookup_action_sender.reserve(), if block_lookup_worker_active
                    && block_lookup_coordinator.pending_action_count() > 0
                    && block_lookup_coordinator.in_flight_action_count() == 0 => {
                    match permit {
                        Ok(permit) => {
                            if let Some(action) = block_lookup_coordinator.next_action() {
                                permit.send(action);
                            }
                        }
                        Err(err) => {
                            block_lookup_worker_active = false;
                            block_lookup_coordinator.fail_in_flight_action();
                            error!("Block lookup worker action channel closed: {err}");
                        }
                    }
                }
                update = block_lookup_update_receiver.recv(), if block_lookup_worker_active => {
                    match update {
                        Some(update) => apply_coordinator_update(
                            &mut block_lookup_coordinator,
                            update,
                        ),
                        None => {
                            block_lookup_worker_active = false;
                            block_lookup_coordinator.fail_in_flight_action();
                            error!("Block lookup worker result channel closed");
                        }
                    }
                }
                import_event = block_import_receiver.recv(), if block_import_receiver_active => {
                    match import_event {
                        Ok(event) => apply_block_import_event(
                            &mut block_lookup_coordinator,
                            event,
                        ),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "Block import notifications lagged; reconciling pending parents");
                            let roots = block_lookup_coordinator.reconciliation_roots();
                            let reconciliation = reconcile_block_lookup_state(
                                &beacon_chain,
                                roots,
                            )
                            .await;
                            for block_root in reconciliation.imported_roots {
                                block_lookup_coordinator.parent_imported(block_root);
                            }
                            for block_root in reconciliation.pending_availability_roots {
                                block_lookup_coordinator
                                    .mark_block_pending_availability(block_root);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            block_import_receiver_active = false;
                            error!("Block import notification channel closed");
                        }
                    }
                }
                result = &mut syncer_handle, if syncer_active => {
                    syncer_active = false;
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
                        syncer_active = true;
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

                    let slots = {
                        let store = beacon_chain.store.lock().await;
                        (
                            store.get_current_slot(),
                            store.db.finalized_checkpoint_provider().get(),
                        )
                    };
                    match slots {
                        (Ok(current_slot), Ok(finalized_checkpoint)) => {
                            block_lookup_coordinator.prune(
                                current_slot,
                                compute_start_slot_at_epoch(finalized_checkpoint.epoch),
                            );
                        }
                        (Err(err), _) => error!("Failed to read current slot: {err}"),
                        (_, Err(err)) => error!("Failed to read finalized checkpoint: {err}"),
                    }
                }
                Some(event) = manager_receiver.recv() => {
                    match event {
                        // Handles Gossipsub messages from other peers.
                        ReamNetworkEvent::GossipsubMessage { propagation_source, message_id, message } => {
                            let mut pending_item = None;
                            let acceptance = handle_gossipsub_message(
                                message,
                                &beacon_chain,
                                &cached_db,
                                &p2p_sender,
                                &mut pending_item,
                            ).await;
                            p2p_sender.report_gossip_validation(
                                message_id,
                                propagation_source,
                                acceptance,
                            );

                            if let Some(item) = pending_item {
                                let block_root = match &item {
                                    PendingGossipItem::Block { block, .. } => {
                                        block.block().message.tree_hash_root()
                                    }
                                    PendingGossipItem::Column { column, .. } => {
                                        column.sidecar().signed_block_header.message.tree_hash_root()
                                    }
                                };
                                let current_slot = {
                                    let store = beacon_chain.store.lock().await;
                                    store.get_current_slot()
                                };
                                match current_slot {
                                    Ok(current_slot) => log_insert_outcome(
                                        block_root,
                                        insert_pending_item(
                                            &mut block_lookup_coordinator,
                                            item,
                                            current_slot,
                                        ),
                                    ),
                                    Err(err) => {
                                        error!("Failed to read current slot for deferred gossip: {err}")
                                    }
                                }
                            }
                        }
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
