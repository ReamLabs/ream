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
    gossipsub::{Event as GossipsubEvent, IdentTopic, MessageAuthenticity},
    identify,
    swarm::{Config, NetworkBehaviour, Swarm, SwarmEvent},
};
use libp2p_identity::{Keypair, PeerId};
use parking_lot::RwLock as ParkingRwLock;
use ream_chain_lean::{lean_chain::LeanChain, service::LeanChainServiceMessage};
use ream_consensus_lean::{QueueItem, VoteItem};
use ream_executor::ReamExecutor;
use ssz::Encode;
use tokio::sync::{
    RwLock,
    mpsc::{UnboundedReceiver, UnboundedSender},
};
use tracing::{info, trace, warn};

use crate::{
    bootnodes::Bootnodes,
    gossipsub::{
        GossipsubBehaviour,
        lean::{
            configurations::LeanGossipsubConfig, message::LeanGossipsubMessage,
            topics::LeanGossipTopicKind,
        },
        snappy::SnappyTransform,
    },
    network::misc::Executor,
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
    chain_message_sender: UnboundedSender<LeanChainServiceMessage>,
    outbound_p2p_request: UnboundedReceiver<QueueItem>,
}

impl LeanNetworkService {
    pub async fn new(
        network_config: Arc<LeanNetworkConfig>,
        lean_chain: Arc<RwLock<LeanChain>>,
        executor: ReamExecutor,
        chain_message_sender: UnboundedSender<LeanChainServiceMessage>,
        outbound_p2p_request: UnboundedReceiver<QueueItem>,
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

        let swarm = {
            let config = Config::with_executor(Executor(executor))
                .with_notify_handler_buffer_size(NonZeroUsize::new(7).expect("Not zero"))
                .with_per_connection_event_buffer_size(4)
                .with_dial_concurrency_factor(NonZeroU8::new(1).unwrap());

            SwarmBuilder::with_existing_identity(local_key.clone())
                .with_tokio()
                .with_quic()
                .with_behaviour(|_| behaviour)?
                .with_swarm_config(|_| config)
                .build()
        };

        let mut lean_network_service = LeanNetworkService {
            lean_chain,
            network_config: network_config.clone(),
            swarm,
            peer_table: ParkingRwLock::new(HashMap::new()),
            chain_message_sender,
            outbound_p2p_request,
        };

        let mut multi_addr: Multiaddr = lean_network_service.network_config.socket_address.into();
        multi_addr.push(Protocol::Udp(
            lean_network_service.network_config.socket_port,
        ));
        multi_addr.push(Protocol::QuicV1);
        info!("Listening on {multi_addr:?}");

        lean_network_service
            .swarm
            .listen_on(multi_addr.clone())
            .map_err(|err| {
                anyhow!("Failed to start libp2p peer listen on {multi_addr:?}, error: {err:?}")
            })?;

        for topic in &network_config.gossipsub_config.topics {
            lean_network_service
                .swarm
                .behaviour_mut()
                .gossipsub
                .subscribe(&IdentTopic::from(topic.clone()))
                .map_err(|err| anyhow!("subscribe to {topic} failed: {err:?}"))?;
        }

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
                Some(item) = self.outbound_p2p_request.recv() => {
                    match item {
                        QueueItem::BlockItem(block) => {
                            if let Err(err) = self.swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(
                                    self.network_config
                                        .gossipsub_config
                                        .topics
                                        .iter()
                                        .find(|block_topic| matches!(block_topic.kind, LeanGossipTopicKind::LeanBlock))
                                        .map(|block_topic| IdentTopic::from(block_topic.clone()))
                                        .expect("LeanBlock topic configured"),
                                    block.as_ssz_bytes(),
                                )
                            {
                                warn!("publish block for slot {} failed: {err:?}", block.slot);
                            } else {
                                info!("broadcasted block for slot {}", block.slot);
                            }
                        }
                        QueueItem::VoteItem(vote) => {
                            let (bytes, slot) = match &vote {
                                VoteItem::Signed(signed) => (signed.as_ssz_bytes(), signed.data.slot),
                                VoteItem::Unsigned(unsigned) => (unsigned.as_ssz_bytes(), unsigned.slot),
                            };

                            if let Err(err) = self.swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(
                                    self.network_config
                                        .gossipsub_config
                                        .topics
                                        .iter()
                                        .find(|vote_topic| matches!(vote_topic.kind, LeanGossipTopicKind::LeanVote))
                                        .map(|vote_topic| IdentTopic::from(vote_topic.clone()))
                                        .expect("LeanVote topic configured"),
                                    bytes,
                                )
                            {
                                warn!("publish vote for slot {slot} failed: {err:?}");
                            } else {
                                info!("broadcasted vote for slot {slot}");
                            }
                        }
                    }
                }

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
            SwarmEvent::Behaviour(ReamBehaviourEvent::Gossipsub(gossipsub_event)) => {
                if let GossipsubEvent::Message { message, .. } = gossipsub_event {
                    match LeanGossipsubMessage::decode(&message.topic, &message.data) {
                        Ok(LeanGossipsubMessage::Block(signed_block)) => {
                            if let Err(err) =
                                self.chain_message_sender.send(LeanChainServiceMessage {
                                    item: QueueItem::BlockItem((signed_block).data.clone()),
                                })
                            {
                                warn!(
                                    "failed to send block for slot {} item to chain: {err:?}",
                                    signed_block.data.slot
                                );
                            }
                        }
                        Ok(LeanGossipsubMessage::Vote(signed_vote)) => {
                            if let Err(err) =
                                self.chain_message_sender.send(LeanChainServiceMessage {
                                    item: QueueItem::VoteItem(VoteItem::Signed(
                                        (*signed_vote).clone(),
                                    )),
                                })
                            {
                                warn!(
                                    "failed to send vote for slot {} to chain: {err:?}",
                                    signed_vote.data.slot
                                );
                            }
                        }
                        Err(err) => warn!("gossip decode failed: {err:?}"),
                    }
                }
                None
            }
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
            SwarmEvent::IncomingConnection { local_addr, .. } => {
                info!("Incoming connection from {local_addr:?}");
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

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, sync::Once, time::Duration};

    use libp2p::{Multiaddr, multiaddr::Protocol};
    use ream_chain_lean::lean_chain::LeanChain;
    use ream_network_spec::networks::{LeanNetworkSpec, set_lean_network_spec};
    use tokio::sync::mpsc;
    use tracing_test::traced_test;

    use super::*;
    use crate::bootnodes::Bootnodes;

    static INIT: Once = Once::new();

    fn ensure_network_spec_init() {
        INIT.call_once(|| {
            set_lean_network_spec(LeanNetworkSpec::default().into());
        });
    }

    pub async fn setup_lean_node(
        socket_port: u16,
    ) -> anyhow::Result<(LeanNetworkService, Multiaddr)> {
        ensure_network_spec_init();

        let lean_chain = Arc::new(RwLock::new(LeanChain::default()));
        let executor = ReamExecutor::new();
        let config = Arc::new(LeanNetworkConfig {
            gossipsub_config: LeanGossipsubConfig::default(),
            socket_address: Ipv4Addr::new(127, 0, 0, 1).into(),
            socket_port,
        });
        let (sender, _receiver) = mpsc::unbounded_channel::<LeanChainServiceMessage>();
        let (_outbound_request_sender_unused, outbound_request_receiver) =
            mpsc::unbounded_channel::<QueueItem>();
        let node = LeanNetworkService::new(
            config.clone(),
            lean_chain,
            executor.unwrap(),
            sender,
            outbound_request_receiver,
        )
        .await?;
        let multi_addr: Multiaddr = config.socket_address.into();
        Ok((node, multi_addr))
    }

    // Test to check connection between 2 QUIC lean nodes
    #[tokio::test]
    #[traced_test]
    async fn test_two_quic_lean_nodes_connection() -> anyhow::Result<()> {
        let socket_port1 = 9000;
        let socket_port2 = 9001;

        let (mut node1, mut node1_addr) = setup_lean_node(socket_port1).await?;
        let (mut node2, _) = setup_lean_node(socket_port2).await?;

        let node1_peer_id = node1.local_peer_id();
        let node2_peer_id = node2.local_peer_id();

        node1_addr.push(Protocol::Udp(socket_port1));
        node1_addr.push(Protocol::QuicV1);
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
