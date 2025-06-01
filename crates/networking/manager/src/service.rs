use std::{path::PathBuf, sync::Arc};

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
use ream_p2p::{
    channel::{P2PMessage, P2PResponse},
    config::NetworkConfig,
    gossipsub::{
        configurations::GossipsubConfig,
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
            status::Status,
        },
    },
};
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use ream_syncer::block_range::BlockRangeSyncer;
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{info, trace, warn};

use crate::config::ManagerConfig;

pub struct ManagerService {
    pub beacon_chain: Arc<BeaconChain>,
    pub manager_receiver: mpsc::UnboundedReceiver<ReamNetworkEvent>,
    p2p_sender: P2PSender,
    pub network_handle: JoinHandle<()>,
    pub network_state: Arc<NetworkState>,
    pub block_range_syncer: BlockRangeSyncer,
    pub ream_db: ReamDB,
}

impl ManagerService {
    pub async fn new(
        async_executor: ReamExecutor,
        config: ManagerConfig,
        ream_db: ReamDB,
        ream_dir: PathBuf,
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
        gossipsub_config.set_topics(vec![GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BeaconBlock,
        }]);

        let network_config = NetworkConfig {
            socket_address: config.socket_address,
            socket_port: config.socket_port,
            discv5_config,
            gossipsub_config,
            data_dir: ream_dir,
        };

        let (manager_sender, manager_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, p2p_receiver) = mpsc::unbounded_channel();

        let network = Network::init(async_executor, &network_config).await?;
        let network_state = network.network_state();
        let network_handle = tokio::spawn(async move {
            network.start(manager_sender, p2p_receiver).await;
        });

        let execution_engine = if let (Some(execution_endpoint), Some(jwt_path)) =
            (config.execution_endpoint, config.execution_jwt_secret)
        {
            Some(ExecutionEngine::new(execution_endpoint, jwt_path)?)
        } else {
            None
        };
        let beacon_chain = Arc::new(BeaconChain::new(ream_db.clone(), execution_engine));
        let block_range_syncer = BlockRangeSyncer::new(beacon_chain.clone(), p2p_sender.clone());

        Ok(Self {
            beacon_chain,
            manager_receiver,
            p2p_sender: P2PSender(p2p_sender),
            network_handle,
            network_state,
            block_range_syncer,
            ream_db,
        })
    }

    pub async fn start(self) {
        let mut manager_receiver = self.manager_receiver;
        loop {
            tokio::select! {
                Some(event) = manager_receiver.recv() => {
                     match event {
                        ReamNetworkEvent::RequestMessage { peer_id, stream_id, connection_id, message } => {
                            match message {
                                RequestMessage::Status(status) => {
                                    trace!(?peer_id, ?stream_id, ?connection_id, ?status, "Received Status request");
                                    let Ok(finalized_checkpoint) = self.ream_db.finalized_checkpoint_provider().get() else {
                                        warn!("Failed to get finalized checkpoint");
                                        self.p2p_sender.send_error_response(
                                            peer_id,
                                            connection_id,
                                            stream_id,
                                            "Failed to get finalized checkpoint",
                                        );
                                        continue;
                                    };

                                    let head_root = match self.beacon_chain.store.get_head() {
                                        Ok(head) => head,
                                        Err(err) => {
                                            warn!("Failed to get head root: {err}, falling back to finalized root");
                                            finalized_checkpoint.root
                                        }
                                    };

                                    let head_slot = match self.ream_db.beacon_block_provider().get(head_root) {
                                        Ok(Some(block)) => block.message.slot,
                                        err => {
                                            warn!("Failed to get block for head root {head_root}: {err:?}");
                                            self.p2p_sender.send_error_response(
                                                peer_id,
                                                connection_id,
                                                stream_id,
                                                &format!("Failed to get block for head root {head_root}: {err:?}"),
                                            );
                                            continue;
                                        }
                                    };

                                    self.p2p_sender.send_response(
                                        peer_id,
                                        connection_id,
                                        stream_id,
                                        ResponseMessage::Status(Status {
                                            fork_digest: network_spec().fork_digest(genesis_validators_root()),
                                            finalized_root: finalized_checkpoint.root,
                                            finalized_epoch: finalized_checkpoint.epoch,
                                            head_root,
                                            head_slot
                                         }),
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
