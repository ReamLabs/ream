use std::{
    collections::HashMap,
    net::IpAddr,
    num::{NonZeroU8, NonZeroUsize},
    sync::Arc,
};

use anyhow::anyhow;
use discv5::multiaddr::Protocol;
use futures::StreamExt;
use libp2p::{
    Multiaddr, SwarmBuilder,
    connection_limits::{self, ConnectionLimits},
    gossipsub::MessageAuthenticity,
    identify,
    swarm::{Config, NetworkBehaviour, Swarm, SwarmEvent},
};
use libp2p_identity::{Keypair, PeerId};
use parking_lot::RwLock as ParkingRwLock;
use ream_chain_lean::lean_chain::LeanChain;
use ream_executor::ReamExecutor;
use tokio::sync::RwLock;
use tracing::{info, trace, warn};

use crate::{
    bootnodes::Bootnodes,
    gossipsub::{
        GossipsubBehaviour, lean::configurations::LeanGossipsubConfig, snappy::SnappyTransform,
    },
    network::misc::{Executor, build_transport},
    peer::ConnectionState,
};

#[derive(NetworkBehaviour)]
pub(crate) struct ReamBehaviour {
    pub identify: identify::Behaviour,

    /// The gossip domain: gossipsub
    pub gossipsub: GossipsubBehaviour,

    pub connection_limits: connection_limits::Behaviour,
}

#[derive(Debug)]
pub enum ReamNetworkEvent {
    PeerConnectedIncoming(PeerId),
    PeerConnectedOutgoing(PeerId),
    PeerDisconnected(PeerId),
    Status(PeerId),
    Ping(PeerId),
    MetaData(PeerId),
    DisconnectPeer(PeerId),
}

pub struct LeanNetworkConfig {
    pub gossipsub_config: LeanGossipsubConfig,
    pub socket_address: IpAddr,
    pub socket_port: u16,
}

/// NetworkService is responsible for the following:
/// 1. Peer management. (We will connect with static peers for PQ devnet.)
/// 2. Gossiping blocks and votes.
///
/// TBD: It will be best if we reuse the existing NetworkManagerService for the beacon node.
pub struct LeanNetworkService {
    lean_chain: Arc<RwLock<LeanChain>>,
    network_config: Arc<LeanNetworkConfig>,
    swarm: Swarm<ReamBehaviour>,
    peer_table: ParkingRwLock<HashMap<PeerId, ConnectionState>>,
}

impl LeanNetworkService {
    pub async fn new(
        network_config: Arc<LeanNetworkConfig>,
        lean_chain: Arc<RwLock<LeanChain>>,
        executor: ReamExecutor,
    ) -> anyhow::Result<Self> {
        let connection_limits = {
            let limits = ConnectionLimits::default()
                .with_max_pending_incoming(Some(5))
                .with_max_pending_outgoing(Some(16))
                .with_max_established_per_peer(Some(1));

            connection_limits::Behaviour::new(limits)
        };

        let local_key = Keypair::generate_secp256k1();

        let gossipsub = {
            let snappy_transform =
                SnappyTransform::new(network_config.gossipsub_config.config.max_transmit_size());
            GossipsubBehaviour::new_with_transform(
                MessageAuthenticity::Anonymous,
                network_config.gossipsub_config.config.clone(),
                None,
                snappy_transform,
            )
            .map_err(|err| anyhow!("Failed to create gossipsub behaviour: {err:?}"))?
        };

        let identify = {
            let local_public_key = local_key.public();
            let identify_config =
                identify::Config::new("eth2/1.0.0".into(), local_public_key.clone())
                    .with_agent_version("0.0.1".to_string())
                    .with_cache_size(0);

            identify::Behaviour::new(identify_config)
        };

        let behaviour = {
            ReamBehaviour {
                gossipsub,
                identify,
                connection_limits,
            }
        };

        let transport = build_transport(local_key.clone())
            .map_err(|err| anyhow!("Failed to build transport: {err:?}"))?;

        let swarm = {
            let config = Config::with_executor(Executor(executor))
                .with_notify_handler_buffer_size(NonZeroUsize::new(7).expect("Not zero"))
                .with_per_connection_event_buffer_size(4)
                .with_dial_concurrency_factor(NonZeroU8::new(1).unwrap());

            let builder = SwarmBuilder::with_existing_identity(local_key.clone())
                .with_tokio()
                .with_other_transport(|_key| transport)
                .expect("initializing swarm");

            builder
                .with_behaviour(|_| behaviour)
                .expect("initializing swarm")
                .with_swarm_config(|_| config)
                .build()
        };

        let mut lean_network_service = LeanNetworkService {
            lean_chain,
            network_config,
            swarm,
            peer_table: ParkingRwLock::new(HashMap::new()),
        };

        let mut multi_addr: Multiaddr = lean_network_service.network_config.socket_address.into();
        multi_addr.push(Protocol::Tcp(
            lean_network_service.network_config.socket_port,
        ));

        lean_network_service
            .swarm
            .listen_on(multi_addr.clone())
            .map_err(|err| {
                anyhow!("Failed to start libp2p peer listen on {multi_addr:?}, error: {err:?}")
            })?;

        Ok(lean_network_service)
    }

    pub async fn start(&mut self, bootnodes: Bootnodes) -> anyhow::Result<()> {
        info!("LeanNetworkService started");
        info!(
            "Current LeanChain head: {}",
            self.lean_chain.read().await.head
        );

        self.connect_to_peers(bootnodes.to_multiaddrs_lean()).await;
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => {
                    if let Some(event) = self.parse_swarm_event(event).await {
                        info!("Swarm event: {event:?}");
                    }
                }
            }
        }
    }

    async fn parse_swarm_event(
        &mut self,
        event: SwarmEvent<ReamBehaviourEvent>,
    ) -> Option<ReamNetworkEvent> {
        match event {
            SwarmEvent::Behaviour(_) => None,
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                self.peer_table
                    .write()
                    .insert(peer_id, ConnectionState::Connected);
                info!("Connected to peer: {peer_id:?}");
                None
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.peer_table
                    .write()
                    .insert(peer_id, ConnectionState::Disconnected);
                info!("Disconnected from peer: {peer_id:?}");
                Some(ReamNetworkEvent::PeerDisconnected(peer_id))
            }
            SwarmEvent::IncomingConnection {
                local_addr,
                send_back_addr,
                ..
            } => {
                info!("Incoming connection from {send_back_addr:?} to {local_addr:?}");
                None
            }
            SwarmEvent::Dialing { peer_id, .. } => {
                info!("Dialing {peer_id:?}");
                Some(ReamNetworkEvent::PeerConnectedOutgoing(peer_id?))
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!("Failed to connect to {peer_id:?}: {error:?}");
                None
            }
            _ => None,
        }
    }

    async fn connect_to_peers(&mut self, peers: Vec<Multiaddr>) {
        trace!("Discovered peers: {peers:?}");
        for peer in peers {
            if let Err(err) = self.swarm.dial(peer.clone()) {
                warn!("Failed to dial peer: {err:?}");
                continue;
            }

            if let Some(Protocol::P2p(peer_id)) = peer
                .iter()
                .find(|protocol| matches!(protocol, Protocol::P2p(_)))
            {
                info!("Dialing peer: {peer_id:?}",);
                self.peer_table
                    .write()
                    .insert(peer_id, ConnectionState::Connecting);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, time::Duration};

    use libp2p::{Multiaddr, multiaddr::Protocol};
    use ream_chain_lean::lean_chain::LeanChain;
    use ream_network_spec::networks::{LeanNetworkSpec, set_lean_network_spec};
    use tracing_test::traced_test;

    use super::*;
    use crate::bootnodes::Bootnodes;

    #[tokio::test]
    #[traced_test]
    async fn test_two_lean_nodes_connection() -> anyhow::Result<()> {
        set_lean_network_spec(LeanNetworkSpec::default().into());

        let lean_chain1 = Arc::new(RwLock::new(LeanChain::default()));
        let lean_chain2 = Arc::new(RwLock::new(LeanChain::default()));

        let executor1 = ReamExecutor::new();
        let executor2 = ReamExecutor::new();

        let config1 = Arc::new(LeanNetworkConfig {
            gossipsub_config: LeanGossipsubConfig::default(),
            socket_address: Ipv4Addr::new(127, 0, 0, 1).into(),
            socket_port: 9000,
        });

        let config2 = Arc::new(LeanNetworkConfig {
            gossipsub_config: LeanGossipsubConfig::default(),
            socket_address: Ipv4Addr::new(127, 0, 0, 1).into(),
            socket_port: 9001,
        });

        let mut node1 =
            LeanNetworkService::new(config1.clone(), lean_chain1, executor1.unwrap()).await?;

        let mut node2 =
            LeanNetworkService::new(config2.clone(), lean_chain2, executor2.unwrap()).await?;

        let node1_peer_id = *node1.swarm.local_peer_id();
        let node2_peer_id = *node2.swarm.local_peer_id();

        let mut node1_addr: Multiaddr = config1.socket_address.into();
        node1_addr.push(Protocol::Tcp(config1.socket_port));
        node1_addr.push(Protocol::P2p(node1_peer_id));

        let node1_handle = tokio::spawn(async move {
            let bootnodes = Bootnodes::Default;

            node1.start(bootnodes).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let node2_handle = tokio::spawn(async move {
            let bootnodes = Bootnodes::Multiaddr(vec![node1_addr]);

            node2.start(bootnodes).await.unwrap();
        });

        tokio::time::sleep(Duration::from_secs(2)).await;

        node1_handle.abort();
        node2_handle.abort();

        assert!(logs_contain(&format!(
            "Dialing peer: PeerId(\"{node1_peer_id}\")"
        )));
        assert!(logs_contain(&format!(
            "Connected to peer: PeerId(\"{node1_peer_id}\")"
        )));
        assert!(logs_contain(&format!(
            "Connected to peer: PeerId(\"{node2_peer_id}\")"
        )));

        Ok(())
    }
}
