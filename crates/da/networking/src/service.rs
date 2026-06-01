use std::sync::Arc;

use alloy_primitives::{B256, aliases::B32};
use anyhow::Result;
use ream_consensus_misc::constants::beacon::NUM_CUSTODY_GROUPS;
use ream_discv5::{
    config::DiscoveryConfig,
    subnet::{AttestationSubnets, CustodyGroupCount, SyncCommitteeSubnets},
};
use ream_executor::ReamExecutor;
use ream_network_manager::p2p_sender::P2PSender;
use ream_p2p::{
    config::NetworkConfig,
    network::beacon::{
        Network, ReamNetworkEvent, network_state::NetworkState, utils::META_DATA_FILE_NAME,
    },
};
use ream_req_resp::beacon::messages::{meta_data::GetMetaDataV3, status::Status};
use ssz::Encode;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{gossip::handle_da_gossip_message, req_resp::handle_da_req_resp_message};
use ream_da_storage::DaStore;

pub struct DaNetworkService {
    network_receiver: mpsc::UnboundedReceiver<ReamNetworkEvent>,
    p2p_sender: P2PSender,
    store: Arc<DaStore>,
    network_state: Arc<NetworkState>,
}

impl DaNetworkService {
    pub async fn new(
        executor: ReamExecutor,
        socket_address: std::net::IpAddr,
        socket_port: u16,
        discovery_port: u16,
        bootnodes: Vec<discv5::Enr>,
        static_peers: Vec<libp2p::Multiaddr>,
        data_dir: std::path::PathBuf,
        store: Arc<DaStore>,
        disable_discovery: bool,
        finalized_root: B256,
        finalized_epoch: u64,
        fork_digest: B32,
    ) -> Result<Self> {
        let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
            socket_address,
            discovery_port,
        ))
        .build();

        let discv5_config = DiscoveryConfig {
            discv5_config,
            bootnodes,
            socket_address,
            socket_port,
            discovery_port,
            disable_discovery,
            attestation_subnets: AttestationSubnets::new(),
            sync_committee_subnets: SyncCommitteeSubnets::new(),
            // Supernode: advertise custody of all 128 groups
            custody_group_count: CustodyGroupCount(NUM_CUSTODY_GROUPS),
            fork_digest_override: Some(fork_digest),
        };

        let gossipsub_config = crate::gossip::da_gossipsub_config(fork_digest);

        let network_config = NetworkConfig {
            discv5_config,
            gossipsub_config,
            data_dir: data_dir.clone(),
        };

        let status = Status {
            fork_digest,
            finalized_root,
            finalized_epoch,
            ..Default::default()
        };

        let (manager_sender, manager_receiver) = mpsc::unbounded_channel();
        let (p2p_sender_tx, p2p_receiver) = mpsc::unbounded_channel();

        let da_meta_data = GetMetaDataV3 {
            seq_number: 0,
            custody_group_count: NUM_CUSTODY_GROUPS as u64,
            ..Default::default()
        };

        let meta_data_path = data_dir.join(META_DATA_FILE_NAME);
        std::fs::create_dir_all(&data_dir)?;
        std::fs::write(&meta_data_path, da_meta_data.as_ssz_bytes())?;

        let mut network = Network::init(executor.clone(), &network_config, status).await?;

        for addr in static_peers {
            network.dial_multiaddr(addr);
        }

        let network_state = network.network_state();

        executor.spawn(async move {
            network.start(manager_sender, p2p_receiver).await;
        });

        Ok(Self {
            network_receiver: manager_receiver,
            p2p_sender: P2PSender(p2p_sender_tx),
            store,
            network_state,
        })
    }

    pub async fn start(mut self) {
        info!("DaNetworkService::start() running");
        loop {
            match self.network_receiver.recv().await {
                Some(ReamNetworkEvent::GossipsubMessage { message }) => {
                    let fork_digest = self.network_state.status.read().fork_digest;
                    handle_da_gossip_message(message, &self.store, &self.p2p_sender, fork_digest)
                        .await;
                }
                Some(ReamNetworkEvent::RequestMessage {
                    peer_id,
                    stream_id,
                    connection_id,
                    message,
                }) => {
                    handle_da_req_resp_message(
                        peer_id,
                        stream_id,
                        connection_id,
                        message,
                        &self.p2p_sender,
                        &self.store,
                    )
                    .await;
                }
                Some(e) => {
                    info!("Ignoring network event: {e:?}");
                } // ignore other events (peer connect/disconnect etc)
                None => {
                    error!("Network receiver closed");
                    break;
                }
            }
        }
    }

    pub fn network_state(&self) -> Arc<NetworkState> {
        self.network_state.clone()
    }
}
