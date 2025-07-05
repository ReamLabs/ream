use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::{blob_sidecar::BlobIdentifier, electra::beacon_state::BeaconState};
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
    gossipsub::message::GossipsubMessage,
    network::{Network, ReamNetworkEvent},
    network_state::NetworkState,
    req_resp::messages::{
        RequestMessage, ResponseMessage,
        beacon_blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request},
        blob_sidecars::{BlobSidecarsByRangeV1Request, BlobSidecarsByRootV1Request},
    },
};
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use ream_syncer::block_range::BlockRangeSyncer;
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info, trace, warn};
use tree_hash::TreeHash;

use crate::{
    config::ManagerConfig, gossipsub::init_gossipsub_config_with_topics, p2p_sender::P2PSender,
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

    /// Starts the manager service, which receives either a Gossipsub message or Req/Resp message
    /// from the network worker, and dispatches them to the appropriate handlers.
    ///
    /// Panics if the manager receiver is not initialized.
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

    /// Starts the manager service, which listens for network events and handles requests.
    ///
    /// Panics if the manager receiver is not initialized.
    pub async fn start(mut self) {
        let network_spec = network_spec();
        let mut slot_interval =
            tokio::time::interval(Duration::from_secs(network_spec.seconds_per_slot));
        let mut expiry_interval = tokio::time::interval(Duration::from_secs(12 * 32));
        loop {
            tokio::select! {
                _ = slot_interval.tick() => {
                    let time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("correct time")
                    .as_secs();
                    if let Err(err) = self.beacon_chain.process_tick(time).await {
                        error!("Failed to process gossipsub tick: {err}");
                    }
                }
                _ = expiry_interval.tick() => {
                    self.check_and_expire_sync_committee_subscriptions().await;
                }
                Some(event) = self.manager_receiver.recv() => {
                    match event {
                        ReamNetworkEvent::GossipsubMessage { message } => {
                            match GossipsubMessage::decode(&message.topic, &message.data) {
                                Ok(gossip_message) => match gossip_message {
                                    GossipsubMessage::BeaconBlock(signed_block) => {
                                        info!(
                                            "Beacon block received over gossipsub: slot: {}, root: {}",
                                            signed_block.message.slot,
                                            signed_block.message.block_root()
                                        );
                                        if let Err(err) = self.beacon_chain.process_block(*signed_block).await {
                                            error!("Failed to process gossipsub beacon block: {err}");
                                        }
                                    }
                                    GossipsubMessage::BeaconAttestation(attestation) => {
                                        info!(
                                            "Beacon Attestation received over gossipsub: root: {}",
                                            attestation.tree_hash_root()
                                        );
                                        if let Err(err) =  self.beacon_chain.process_attestation(*attestation, true).await {
                                            error!("Failed to process gossipsub attestation: {err}");
                                        }
                                    }
                                    GossipsubMessage::BlsToExecutionChange(bls_to_execution_change) => {
                                        info!(
                                            "Bls To Execution Change received over gossipsub: root: {}",
                                            bls_to_execution_change.tree_hash_root()
                                        );
                                    }
                                    GossipsubMessage::AggregateAndProof(aggregate_and_proof) => {
                                        info!(
                                            "Aggregate And Proof received over gossipsub: root: {}",
                                            aggregate_and_proof.tree_hash_root()
                                        );
                                    }
                                    GossipsubMessage::SyncCommittee(sync_committee) => {
                                        info!(
                                            "Sync Committee received over gossipsub: root: {}",
                                            sync_committee.tree_hash_root()
                                        );
                                    }
                                    GossipsubMessage::SyncCommitteeContributionAndProof(
                                        sync_committee_contribution_and_proof,
                                    ) => {
                                        info!(
                                            "Sync Committee Contribution And Proof received over gossipsub: root: {}",
                                            sync_committee_contribution_and_proof.tree_hash_root()
                                        );
                                    }
                                    GossipsubMessage::AttesterSlashing(attester_slashing) => {
                                        info!(
                                            "Attester Slashing received over gossipsub: root: {}",
                                            attester_slashing.tree_hash_root()
                                        );

                                        if let Err(err) = self.beacon_chain.process_attester_slashing(*attester_slashing).await {
                                            error!("Failed to process gossipsub attester slashing: {err}");
                                        }
                                    }
                                    GossipsubMessage::ProposerSlashing(proposer_slashing) => {
                                        info!(
                                            "Proposer Slashing received over gossipsub: root: {}",
                                            proposer_slashing.tree_hash_root()
                                        );
                                    }
                                    GossipsubMessage::BlobSidecar(blob_sidecar) => {
                                        info!(
                                            "Blob Sidecar received over gossipsub: root: {}",
                                            blob_sidecar.tree_hash_root()
                                        );
                                    }
                                    GossipsubMessage::LightClientFinalityUpdate(light_client_finality_update) => {
                                        info!(
                                            "Light Client Finality Update received over gossipsub: root: {}",
                                            light_client_finality_update.tree_hash_root()
                                        );
                                    }
                                    GossipsubMessage::LightClientOptimisticUpdate(
                                        light_client_optimistic_update,
                                    ) => {
                                        info!(
                                            "Light Client Optimistic Update received over gossipsub: root: {}",
                                            light_client_optimistic_update.tree_hash_root()
                                        );
                                    }
                                },
                                Err(err) => {
                                    trace!("Failed to decode gossip message: {err:?}");
                                }
                            }
                        },
                        ReamNetworkEvent::RequestMessage { peer_id, stream_id, connection_id, message } => {
                            match message {
                                RequestMessage::Status(status) => {
                                    trace!(?peer_id, ?stream_id, ?connection_id, ?status, "Received Status request");
                                    let status = match self.beacon_chain.build_status_request().await {
                                        Ok(status) => status,
                                        Err(err) => {
                                            warn!("Failed to build status request: {err}");
                                            let finalized_checkpoint = match self.ream_db.finalized_checkpoint_provider().get() {
                                                Ok(checkpoint) => checkpoint,
                                                Err(e) => {
                                                    warn!("Failed to get finalized checkpoint: {e}");
                                                    self.p2p_sender.send_error_response(
                                                        peer_id,
                                                        connection_id,
                                                        stream_id,
                                                        "Failed to get finalized checkpoint",
                                                    );
                                                    continue;
                                                }
                                            };

                                            let head_root = match self.beacon_chain.store.lock().await.get_head() {
                                                Ok(head) => head,
                                                Err(err) => {
                                                    warn!("Failed to get head root: {err}, falling back to finalized root");
                                                    finalized_checkpoint.root
                                                }
                                            };

                                            let _head_slot = match self.ream_db.beacon_block_provider().get(head_root) {
                                                Ok(Some(block)) => block.message.slot,
                                                err => {
                                                    warn!("Failed to get block for head root {head_root}: {err:?}");
                                                    self.p2p_sender.send_error_response(
                                                        peer_id,
                                                        connection_id,
                                                        stream_id,
                                                        &format!("Failed to build status request: {err:?}")
                                                    );
                                                    continue;
                                                }
                                            };

                                            continue;
                                        }
                                    };
                                     self.p2p_sender.send_response(
                                        peer_id,
                                        connection_id,
                                        stream_id,
                                        ResponseMessage::Status(status),
                                    );

                                    self.p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BeaconBlocksByRange(BeaconBlocksByRangeV2Request { start_slot, count, .. }) => {
                                    for slot in start_slot..start_slot + count {
                                        let Ok(Some(block_root)) = self.ream_db.slot_index_provider().get(slot) else {
                                            trace!("No block root found for slot {slot}");
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block root found for slot {slot}"),
                                            );
                                            continue;
                                        };
                                        let Ok(Some(block)) = self.ream_db.beacon_block_provider().get(block_root) else {
                                            trace!("No block found for root {block_root}");
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block found for root {block_root}"),
                                            );
                                            continue;
                                        };

                                        self.p2p_sender.send_response(
                                            peer_id,
                                            connection_id,
                                            stream_id,
                                            ResponseMessage::BeaconBlocksByRange(block),
                                        );
                                    }

                                    self.p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BeaconBlocksByRoot(BeaconBlocksByRootV2Request { inner }) =>
                                {
                                    for block_root in inner {
                                        let Ok(Some(block)) = self.ream_db.beacon_block_provider().get(block_root) else {
                                            trace!("No block found for root {block_root}");
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block found for root {block_root}"),
                                            );
                                            continue;
                                        };

                                        self.p2p_sender.send_response(
                                            peer_id,
                                            connection_id,
                                            stream_id,
                                            ResponseMessage::BeaconBlocksByRoot(block),
                                        );
                                    }

                                    self.p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BlobSidecarsByRange(BlobSidecarsByRangeV1Request { start_slot, count }) => {
                                    for slot in start_slot..start_slot + count {
                                        let Ok(Some(block_root)) = self.ream_db.slot_index_provider().get(slot) else {
                                            trace!("No block root found for slot {slot}");
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block root found for slot {slot}"),
                                            );
                                            continue;
                                        };
                                        let Ok(Some(block)) = self.ream_db.beacon_block_provider().get(block_root) else {
                                            trace!("No block found for root {block_root}");
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block found for root {block_root}"),
                                            );
                                            continue;
                                        };

                                        for index in 0..block.message.body.blob_kzg_commitments.len() {
                                            let Ok(Some(blob_and_proof)) = self.ream_db.blobs_and_proofs_provider().get(BlobIdentifier::new(block_root, index as u64)) else {
                                                trace!("No blob and proof found for block root {block_root} and index {index}");
                                                self.p2p_sender.send_error_response(
                                                    peer_id,
                                                    connection_id,
                                                    stream_id,
                                                    &format!("No blob and proof found for block root {block_root} and index {index}"),
                                                );
                                                continue;
                                            };

                                            let blob_sidecar = match block.blob_sidecar(blob_and_proof, index as u64) {
                                                Ok(blob_sidecar) => blob_sidecar,
                                                Err(err) => {
                                                    info!("Failed to create blob sidecar for block root {block_root} and index {index}: {err}");
                                                    self.p2p_sender.send_error_response(
                                                        peer_id,
                                                        connection_id,
                                                        stream_id,
                                                        &format!("Failed to create blob sidecar: {err}"),
                                                    );
                                                    continue;
                                                }
                                            };

                                            self.p2p_sender.send_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                ResponseMessage::BlobSidecarsByRange(blob_sidecar),
                                            );
                                        }
                                    }

                                    self.p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BlobSidecarsByRoot(BlobSidecarsByRootV1Request { inner }) => {
                                    for blob_identifier in inner {
                                        let Ok(Some(blob_and_proof)) = self.ream_db.blobs_and_proofs_provider().get(blob_identifier.clone()) else {
                                            trace!("No blob and proof found for identifier {blob_identifier:?}");
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No blob and proof found for identifier {blob_identifier:?}"),
                                            );
                                            continue;
                                        };

                                        let Ok(Some(block)) = self.ream_db.beacon_block_provider().get(blob_identifier.block_root) else {
                                            trace!("No block found for root {}", blob_identifier.block_root);
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block found for root {}", blob_identifier.block_root),
                                            );
                                            continue;
                                        };

                                        let blob_sidecar = match block.blob_sidecar(blob_and_proof, blob_identifier.index) {
                                            Ok(blob_sidecar) => blob_sidecar,
                                            Err(err) => {
                                                info!("Failed to create blob sidecar for identifier {blob_identifier:?}: {err}");
                                                self.p2p_sender.send_error_response(
                                                    peer_id,
                                                    connection_id,
                                                    stream_id,
                                                    &format!("Failed to create blob sidecar: {err}"),
                                                );
                                                continue;
                                            }
                                        };

                                        self.p2p_sender.send_response(
                                            peer_id,
                                            connection_id,
                                            stream_id,
                                            ResponseMessage::BlobSidecarsByRoot(blob_sidecar),
                                        );
                                    }
                                    self.p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                _ => warn!("This message shouldn't be handled in the network manager: {message:?}"),
                            }
                        },
                        unhandled_request => {
                            info!("Unhandled request: {unhandled_request:?}");
                        }
                    }
                }
            }
        }
    }
}
