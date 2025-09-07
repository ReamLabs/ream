use std::{
    collections::HashMap,
    fs,
    net::IpAddr,
    num::{NonZeroU8, NonZeroUsize},
    sync::Arc,
};

use anyhow::{anyhow, ensure};
use discv5::multiaddr::Protocol;
use futures::StreamExt;
use libp2p::{
    Multiaddr, SwarmBuilder,
    connection_limits::{self, ConnectionLimits},
    gossipsub::{Event as GossipsubEvent, IdentTopic, MessageAuthenticity},
    identify,
    swarm::{Config, NetworkBehaviour, Swarm, SwarmEvent},
};
use libp2p_identity::{KeyType, Keypair, PeerId};
use parking_lot::Mutex;
use ream_chain_lean::{
    lean_chain::LeanChainReader, messages::LeanChainServiceMessage, p2p_request::LeanP2PRequest,
};
use ream_executor::ReamExecutor;
use ssz::Encode;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info, trace, warn};

use super::peer::ConnectionState;
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
    req_resp::{Chain, ReqResp, ReqRespMessage},
};

#[derive(NetworkBehaviour)]
pub(crate) struct ReamBehaviour {
    pub identify: identify::Behaviour,

    /// The request-response domain
    pub req_resp: ReqResp,

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
    pub private_key_path: Option<std::path::PathBuf>,
}

/// NetworkService is responsible for the following:
/// 1. Peer management. (We will connect with static peers for PQ devnet.)
/// 2. Gossiping blocks and votes.
///
/// TBD: It will be best if we reuse the existing NetworkManagerService for the beacon node.
pub struct LeanNetworkService {
    lean_chain: LeanChainReader,
    network_config: Arc<LeanNetworkConfig>,
    swarm: Swarm<ReamBehaviour>,
    peer_table: Arc<Mutex<HashMap<PeerId, ConnectionState>>>,
    chain_message_sender: UnboundedSender<LeanChainServiceMessage>,
    outbound_p2p_request: UnboundedReceiver<LeanP2PRequest>,
}

impl LeanNetworkService {
    pub async fn new(
        network_config: Arc<LeanNetworkConfig>,
        lean_chain: LeanChainReader,
        executor: ReamExecutor,
        chain_message_sender: UnboundedSender<LeanChainServiceMessage>,
        outbound_p2p_request: UnboundedReceiver<LeanP2PRequest>,
    ) -> anyhow::Result<Self> {
        let connection_limits = {
            let limits = ConnectionLimits::default()
                .with_max_pending_incoming(Some(5))
                .with_max_pending_outgoing(Some(16))
                .with_max_established_per_peer(Some(1));

            connection_limits::Behaviour::new(limits)
        };

        let local_key = if let Some(ref path) = network_config.private_key_path {
            let bytes = fs::read(path).map_err(|err| {
                anyhow!("failed to read secret key file {}: {err}", path.display())
            })?;

            let keypair = Keypair::from_protobuf_encoding(&bytes).map_err(|err| {
                anyhow!("failed to decode protobuf encoded libp2p keypair: {err}")
            })?;

            ensure!(
                keypair.key_type() == KeyType::Secp256k1,
                "provided keypair that is not secp256k1"
            );

            keypair
        } else {
            Keypair::generate_secp256k1()
        };

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
                req_resp: ReqResp::new(Chain::Lean),
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
            peer_table: Arc::new(Mutex::new(HashMap::new())),
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
                        LeanP2PRequest::GossipBlock(signed_block) => {
                            if let Err(err) = self.swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(
                                    self.network_config
                                        .gossipsub_config
                                        .topics
                                        .iter()
                                        .find(|block_topic| matches!(block_topic.kind, LeanGossipTopicKind::Block))
                                        .map(|block_topic| IdentTopic::from(block_topic.clone()))
                                        .expect("LeanBlock topic configured"),
                                    signed_block.as_ssz_bytes(),
                                )
                            {
                                warn!("publish block for slot {} failed: {err:?}", signed_block.message.slot);
                            } else {
                                info!("broadcasted block for slot {}", signed_block.message.slot);
                            }
                        }
                        LeanP2PRequest::GossipVote(signed_vote) => {
                            if let Err(err) = self.swarm
                                .behaviour_mut()
                                .gossipsub
                                .publish(
                                    self.network_config
                                        .gossipsub_config
                                        .topics
                                        .iter()
                                        .find(|vote_topic| matches!(vote_topic.kind, LeanGossipTopicKind::Vote))
                                        .map(|vote_topic| IdentTopic::from(vote_topic.clone()))
                                        .expect("LeanVote topic configured"),
                                    signed_vote.as_ssz_bytes(),
                                )
                            {
                                warn!("publish vote for slot {} failed: {err:?}", signed_vote.data.slot);
                            } else {
                                info!("broadcasted vote for slot {}", signed_vote.data.slot);
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
                self.handle_gossipsub_event(gossipsub_event)
            }
            SwarmEvent::Behaviour(ReamBehaviourEvent::ReqResp(req_resp_event)) => {
                self.handle_request_response_event(req_resp_event)
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                self.peer_table
                    .lock()
                    .insert(peer_id, ConnectionState::Connected);

                info!("Connected to peer: {peer_id:?}");
                None
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.peer_table
                    .lock()
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

    fn handle_gossipsub_event(&mut self, event: GossipsubEvent) -> Option<ReamNetworkEvent> {
        if let GossipsubEvent::Message { message, .. } = event {
            match LeanGossipsubMessage::decode(&message.topic, &message.data) {
                Ok(LeanGossipsubMessage::Block(signed_block)) => {
                    let slot = signed_block.message.slot;

                    if let Err(err) =
                        self.chain_message_sender
                            .send(LeanChainServiceMessage::ProcessBlock {
                                signed_block: *signed_block,
                                is_trusted: false,
                                need_gossip: true,
                            })
                    {
                        warn!("failed to send block for slot {slot} item to chain: {err:?}");
                    }
                }
                Ok(LeanGossipsubMessage::Vote(signed_vote)) => {
                    let slot = signed_vote.data.slot;

                    if let Err(err) =
                        self.chain_message_sender
                            .send(LeanChainServiceMessage::ProcessVote {
                                signed_vote: *signed_vote,
                                is_trusted: false,
                                need_gossip: true,
                            })
                    {
                        warn!("failed to send vote for slot {slot} to chain: {err:?}");
                    }
                }
                Err(err) => warn!("gossip decode failed: {err:?}"),
            }
        }
        None
    }

    fn handle_request_response_event(
        &mut self,
        _event: ReqRespMessage,
    ) -> Option<ReamNetworkEvent> {
        None
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
                    .lock()
                    .insert(peer_id, ConnectionState::Connecting);
            }
        }
    }

    pub fn peer_table(&self) -> Arc<Mutex<HashMap<PeerId, ConnectionState>>> {
        self.peer_table.clone()
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, sync::Once, time::Duration};

    use alloy_primitives::B256;
    use libp2p::{Multiaddr, multiaddr::Protocol};
    use ream_chain_lean::lean_chain::LeanChain;
    use ream_network_spec::networks::{LeanNetworkSpec, set_lean_network_spec};
    use ream_storage::db::ReamDB;
    use ream_sync::rwlock::Writer;
    use tempdir::TempDir;
    use tokio::sync::{Mutex, mpsc};
    use tracing_test::traced_test;

    use super::*;
    use crate::bootnodes::Bootnodes;

    static INIT: Once = Once::new();

    fn ensure_network_spec_init() {
        INIT.call_once(|| {
            set_lean_network_spec(LeanNetworkSpec::default().into());
        });
    }

    fn create_lean_chain() -> LeanChain {
        let temp_dir = TempDir::new("lean_node_test").unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        let ream_db = ReamDB::new(temp_path).expect("unable to init Ream Database");
        let lean_db = ream_db
            .init_lean_db()
            .expect("unable to init Ream Lean Database");
        LeanChain {
            store: Arc::new(Mutex::new(lean_db)),
            chain: HashMap::new(),
            post_states: HashMap::new(),
            known_votes: Vec::new(),
            head: B256::default(),
            safe_target: B256::default(),
            new_votes: Vec::new(),
            genesis_hash: B256::default(),
            num_validators: 0,
        }
    }

    pub async fn setup_lean_node(
        socket_port: u16,
    ) -> anyhow::Result<(LeanNetworkService, Multiaddr)> {
        ensure_network_spec_init();

        let (_, lean_chain_reader) = Writer::new(create_lean_chain());
        let executor = ReamExecutor::new().expect("Failed to create executor");
        let config = Arc::new(LeanNetworkConfig {
            gossipsub_config: LeanGossipsubConfig::default(),
            socket_address: Ipv4Addr::new(127, 0, 0, 1).into(),
            socket_port,
            private_key_path: None,
        });
        let (sender, _receiver) = mpsc::unbounded_channel::<LeanChainServiceMessage>();
        let (_outbound_request_sender_unused, outbound_request_receiver) =
            mpsc::unbounded_channel::<LeanP2PRequest>();
        let node = LeanNetworkService::new(
            config.clone(),
            lean_chain_reader,
            executor,
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
