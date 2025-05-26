use std::sync::Arc;

use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::constants::genesis_validators_root;
use ream_discv5::{
    config::DiscoveryConfig,
    subnet::{AttestationSubnets, SyncCommitteeSubnets},
};
use ream_execution_engine::ExecutionEngine;
use ream_executor::ReamExecutor;
use ream_network_spec::networks::network_spec;
use ream_p2p::{
    channel::P2PMessages,
    config::NetworkConfig,
    gossipsub::{
        configurations::GossipsubConfig,
        topics::{GossipTopic, GossipTopicKind},
    },
    network::{Network, PeerTable, ReamNetworkEvent},
};
use ream_storage::db::ReamDB;
use ream_syncer::block_range::BlockRangeSyncer;
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::info;

use crate::config::ManagerConfig;

pub struct ManagerService {
    pub manager_receiver: mpsc::UnboundedReceiver<ReamNetworkEvent>,
    pub p2p_sender: mpsc::UnboundedSender<P2PMessages>,
    pub network_handle: JoinHandle<()>,
    pub peer_table: PeerTable,
    pub block_range_syncer: BlockRangeSyncer,
}

impl ManagerService {
    pub async fn new(
        async_executor: ReamExecutor,
        config: ManagerConfig,
        ream_db: ReamDB,
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
        };

        let (manager_sender, manager_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, p2p_receiver) = mpsc::unbounded_channel();

        let network = Network::init(async_executor, &network_config).await?;
        let peer_table = network.peer_table().clone();
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
        let beacon_chain = Arc::new(BeaconChain::new(ream_db, execution_engine));
        let block_range_syncer = BlockRangeSyncer::new(beacon_chain, p2p_sender.clone());

        Ok(Self {
            manager_receiver,
            p2p_sender,
            network_handle,
            peer_table,
            block_range_syncer,
        })
    }

    pub async fn start(self) {
        let mut manager_receiver = self.manager_receiver;
        loop {
            tokio::select! {
                Some(event) = manager_receiver.recv() => {
                     info!("Received event: {:?}", event);
                }
            }
        }
    }

    pub fn peer_table(&self) -> &PeerTable {
        &self.peer_table
    }
}
