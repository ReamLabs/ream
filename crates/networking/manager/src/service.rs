use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use discv5::multiaddr::PeerId;
use libp2p::swarm::ConnectionId;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::{blob_sidecar::BlobIdentifier, constants::genesis_validators_root};
use ream_discv5::{
    config::DiscoveryConfig,
    subnet::{AttestationSubnets, SyncCommitteeSubnets},
};
use ream_execution_engine::ExecutionEngine;
use ream_executor::ReamExecutor;
use ream_network_spec::networks::network_spec;
use ream_operation_pool::OperationPool;
use ream_p2p::{
    channel::{P2PMessage, P2PResponse},
    config::NetworkConfig,
    gossipsub::{
        configurations::GossipsubConfig,
        message::GossipsubMessage,
        topics::{GossipTopic, GossipTopicKind},
    },
    network::{Network, ReamNetworkEvent},
    network_state::NetworkState,
    req_resp::{
        error::ReqRespError,
        handler::RespMessage,
        messages::{
            RequestMessage, ResponseMessage,
            beacon_blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request},
            blob_sidecars::{BlobSidecarsByRangeV1Request, BlobSidecarsByRootV1Request},
        },
    },
};
use ream_storage::{db::ReamDB, tables::Table};
use ream_syncer::block_range::BlockRangeSyncer;
use tokio::{sync::mpsc, time::interval};
use tracing::{error, info, trace, warn};
use tree_hash::TreeHash;

use crate::config::ManagerConfig;

pub struct ManagerService {
    pub beacon_chain: Arc<BeaconChain>,
    manager_receiver: mpsc::UnboundedReceiver<ReamNetworkEvent>,
    p2p_sender: P2PSender,
    pub network_state: Arc<NetworkState>,
    pub block_range_syncer: BlockRangeSyncer,
    pub ream_db: ReamDB,
}

impl ManagerService {
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

        let mut gossipsub_config = GossipsubConfig::default();
        gossipsub_config.set_topics(vec![
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::BeaconBlock,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::AggregateAndProof,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::VoluntaryExit,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::ProposerSlashing,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::AttesterSlashing,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::BeaconAttestation(0),
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::SyncCommittee(0),
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::SyncCommitteeContributionAndProof,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::BlsToExecutionChange,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::LightClientFinalityUpdate,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::LightClientOptimisticUpdate,
            },
            GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::BlobSidecar(0),
            },
        ]);

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

        Ok(Self {
            beacon_chain,
            manager_receiver,
            p2p_sender: P2PSender(p2p_sender),
            network_state,
            block_range_syncer,
            ream_db,
        })
    }

    /// Starts the manager service, which listens for network events and handles requests.
    ///
    /// Panics if the manager receiver is not initialized.
    pub async fn start(self) {
        let ManagerService {
            beacon_chain,
            mut manager_receiver,
            p2p_sender,
            ream_db,
            ..
        } = self;
        let mut interval = interval(Duration::from_secs(network_spec().seconds_per_slot));
        loop {
            tokio::select! {
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
                        ReamNetworkEvent::GossipsubMessage { message } => {
                            match GossipsubMessage::decode(&message.topic, &message.data) {
                                Ok(gossip_message) => match gossip_message {
                                    GossipsubMessage::BeaconBlock(signed_block) => {
                                        info!(
                                            "Beacon block received over gossipsub: slot: {}, root: {}",
                                            signed_block.message.slot,
                                            signed_block.message.block_root()
                                        );

                                        if let Err(err) =  beacon_chain.process_block(*signed_block).await {
                                            error!("Failed to process gossipsub beacon block: {err}");
                                        }
                                    }
                                    GossipsubMessage::BeaconAttestation(attestation) => {
                                        info!(
                                            "Beacon Attestation received over gossipsub: root: {}",
                                            attestation.tree_hash_root()
                                        );

                                        if let Err(err) =  beacon_chain.process_attestation(*attestation, true).await {
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

                                        if let Err(err) = beacon_chain.process_attester_slashing(*attester_slashing).await {
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
                                    let status = match beacon_chain.build_status_request().await {
                                        Ok(status) => status,
                                        Err(err) => {
                                            warn!("Failed to build status request: {err}");
                                            p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("Failed to build status request: {err}"),
                                            );
                                            continue;
                                        }
                                    };

                                     p2p_sender.send_response(
                                        peer_id,
                                        connection_id,
                                        stream_id,
                                        ResponseMessage::Status(status),
                                    );

                                    p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BeaconBlocksByRange(BeaconBlocksByRangeV2Request { start_slot, count, .. }) => {
                                    for slot in start_slot..start_slot + count {
                                        let Ok(Some(block_root)) = ream_db.slot_index_provider().get(slot) else {
                                            trace!("No block root found for slot {slot}");
                                            p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block root found for slot {slot}"),
                                            );
                                            continue;
                                        };
                                        let Ok(Some(block)) = ream_db.beacon_block_provider().get(block_root) else {
                                            trace!("No block found for root {block_root}");
                                            p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block found for root {block_root}"),
                                            );
                                            continue;
                                        };

                                        p2p_sender.send_response(
                                            peer_id,
                                            connection_id,
                                            stream_id,
                                            ResponseMessage::BeaconBlocksByRange(block),
                                        );
                                    }

                                    p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BeaconBlocksByRoot(BeaconBlocksByRootV2Request { inner }) =>
                                {
                                    for block_root in inner {
                                        let Ok(Some(block)) = ream_db.beacon_block_provider().get(block_root) else {
                                            trace!("No block found for root {block_root}");
                                            p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block found for root {block_root}"),
                                            );
                                            continue;
                                        };

                                        p2p_sender.send_response(
                                            peer_id,
                                            connection_id,
                                            stream_id,
                                            ResponseMessage::BeaconBlocksByRoot(block),
                                        );
                                    }

                                    p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BlobSidecarsByRange(BlobSidecarsByRangeV1Request { start_slot, count }) => {
                                    for slot in start_slot..start_slot + count {
                                        let Ok(Some(block_root)) = ream_db.slot_index_provider().get(slot) else {
                                            trace!("No block root found for slot {slot}");
                                            p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block root found for slot {slot}"),
                                            );
                                            continue;
                                        };
                                        let Ok(Some(block)) = ream_db.beacon_block_provider().get(block_root) else {
                                            trace!("No block found for root {block_root}");
                                            p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No block found for root {block_root}"),
                                            );
                                            continue;
                                        };

                                        for index in 0..block.message.body.blob_kzg_commitments.len() {
                                            let Ok(Some(blob_and_proof)) = ream_db.blobs_and_proofs_provider().get(BlobIdentifier::new(block_root, index as u64)) else {
                                                trace!("No blob and proof found for block root {block_root} and index {index}");
                                                p2p_sender.send_error_response(
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
                                                    p2p_sender.send_error_response(
                                                        peer_id,
                                                        connection_id,
                                                        stream_id,
                                                        &format!("Failed to create blob sidecar: {err}"),
                                                    );
                                                    continue;
                                                }
                                            };

                                            p2p_sender.send_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                ResponseMessage::BlobSidecarsByRange(blob_sidecar),
                                            );
                                        }
                                    }

                                    p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
                                },
                                RequestMessage::BlobSidecarsByRoot(BlobSidecarsByRootV1Request { inner }) => {
                                    for blob_identifier in inner {
                                        let Ok(Some(blob_and_proof)) = ream_db.blobs_and_proofs_provider().get(blob_identifier.clone()) else {
                                            trace!("No blob and proof found for identifier {blob_identifier:?}");
                                            p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("No blob and proof found for identifier {blob_identifier:?}"),
                                            );
                                            continue;
                                        };

                                        let Ok(Some(block)) = ream_db.beacon_block_provider().get(blob_identifier.block_root) else {
                                            trace!("No block found for root {}", blob_identifier.block_root);
                                            p2p_sender.send_error_response(
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
                                                p2p_sender.send_error_response(
                                                    peer_id,
                                                    connection_id,
                                                    stream_id,
                                                    &format!("Failed to create blob sidecar: {err}"),
                                                );
                                                continue;
                                            }
                                        };

                                        p2p_sender.send_response(
                                            peer_id,
                                            connection_id,
                                            stream_id,
                                            ResponseMessage::BlobSidecarsByRoot(blob_sidecar),
                                        );
                                    }
                                    p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
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

struct P2PSender(pub mpsc::UnboundedSender<P2PMessage>);

impl P2PSender {
    pub fn send_response(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
        message: ResponseMessage,
    ) {
        if let Err(err) = self.0.send(P2PMessage::Response(P2PResponse {
            peer_id,
            connection_id,
            stream_id,
            message: RespMessage::Response(Box::new(message)),
        })) {
            warn!("Failed to send P2P response: {err}");
        }
    }

    pub fn send_end_of_stream_response(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
    ) {
        if let Err(err) = self.0.send(P2PMessage::Response(P2PResponse {
            peer_id,
            connection_id,
            stream_id,
            message: RespMessage::EndOfStream,
        })) {
            warn!("Failed to send end of stream response: {err}");
        }
    }

    pub fn send_error_response(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
        error: &str,
    ) {
        if let Err(err) = self.0.send(P2PMessage::Response(P2PResponse {
            peer_id,
            connection_id,
            stream_id,
            message: RespMessage::Error(ReqRespError::Anyhow(anyhow!(error.to_string()))),
        })) {
            warn!("Failed to send error response: {err}");
        }
    }
}
