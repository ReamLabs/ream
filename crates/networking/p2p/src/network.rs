use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    io,
    num::{NonZeroU8, NonZeroUsize},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use discv5::Enr;
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder, Transport, connection_limits,
    core::{
        muxing::StreamMuxerBox,
        transport::Boxed,
        upgrade::{SelectUpgrade, Version},
    },
    dns::Transport as DnsTransport,
    futures::StreamExt,
    gossipsub::{Event as GossipsubEvent, IdentTopic as Topic, MessageAuthenticity},
    identify,
    multiaddr::Protocol,
    noise::Config as NoiseConfig,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp::{Config as TcpConfig, tokio::Transport as TcpTransport},
    yamux,
};
use libp2p_identity::{Keypair, PublicKey, secp256k1};
use libp2p_mplex::{MaxBufferBehaviour, MplexConfig};
use parking_lot::Mutex;
use ream_discv5::discovery::{DiscoveredPeers, Discovery};
use ream_executor::ReamExecutor;
use tracing::{error, info, trace, warn};
use yamux::Config as YamuxConfig;

use crate::{
    config::NetworkConfig,
    gossipsub::{GossipsubBehaviour, snappy::SnappyTransform, topics::GossipTopic},
    req_resp::ReqResp,
};

#[derive(NetworkBehaviour)]
pub(crate) struct ReamBehaviour {
    pub identify: identify::Behaviour,

    /// The discovery domain: discv5
    pub discovery: Discovery,

    /// The request-response domain
    pub req_resp: ReqResp,

    /// The gossip domain: gossipsub
    pub gossipsub: GossipsubBehaviour,

    pub connection_registry: connection_limits::Behaviour,
}

// TODO: these are stub events which needs to be replaced
#[derive(Debug)]
pub enum ReamNetworkEvent {
    PeerConnectedIncoming(PeerId),
    PeerConnectedOutgoing(PeerId),
    PeerDisconnected(PeerId),
    Status(PeerId),
    Ping(PeerId),
    MetaData(PeerId),
    DisconnectPeer(PeerId),
    DiscoverPeers(usize),
}

pub struct Network {
    peer_id: PeerId,
    swarm: Swarm<ReamBehaviour>,
    subscribed_topics: Arc<Mutex<HashSet<GossipTopic>>>,
}

struct Executor(ReamExecutor);

impl libp2p::swarm::Executor for Executor {
    fn exec(&self, f: Pin<Box<dyn futures::Future<Output = ()> + Send>>) {
        self.0.spawn(f);
    }
}

impl Network {
    pub async fn init(executor: ReamExecutor, config: &NetworkConfig) -> anyhow::Result<Self> {
        let local_key = secp256k1::Keypair::generate();

        let discovery = {
            let mut discovery =
                Discovery::new(Keypair::from(local_key.clone()), &config.discv5_config).await?;
            discovery.discover_peers(16, None);
            discovery
        };

        let req_resp = ReqResp::new();

        let gossipsub = {
            let snappy_transform =
                SnappyTransform::new(config.gossipsub_config.config.max_transmit_size());
            GossipsubBehaviour::new_with_transform(
                MessageAuthenticity::Anonymous,
                config.gossipsub_config.config.clone(),
                None,
                snappy_transform,
            )
            .map_err(|err| anyhow!("Failed to create gossipsub behaviour: {err:?}"))?
        };

        let connection_limits = {
            let limits = libp2p::connection_limits::ConnectionLimits::default()
                .with_max_pending_incoming(Some(5))
                .with_max_pending_outgoing(Some(16))
                .with_max_established_per_peer(Some(1));

            libp2p::connection_limits::Behaviour::new(limits)
        };

        let identify = {
            let local_public_key = local_key.public();
            let identify_config = identify::Config::new(
                "eth2/1.0.0".into(),
                PublicKey::from(local_public_key.clone()),
            )
            .with_agent_version("0.0.1".to_string())
            .with_cache_size(0);

            identify::Behaviour::new(identify_config)
        };

        let behaviour = {
            ReamBehaviour {
                discovery,
                req_resp,
                gossipsub,
                identify,
                connection_registry: connection_limits,
            }
        };

        let transport = build_transport(Keypair::from(local_key.clone()))
            .map_err(|err| anyhow!("Failed to build transport: {err:?}"))?;

        let swarm = {
            let config = libp2p::swarm::Config::with_executor(Executor(executor))
                .with_notify_handler_buffer_size(NonZeroUsize::new(7).expect("Not zero"))
                .with_per_connection_event_buffer_size(4)
                .with_dial_concurrency_factor(NonZeroU8::new(1).unwrap());

            let builder = SwarmBuilder::with_existing_identity(Keypair::from(local_key.clone()))
                .with_tokio()
                .with_other_transport(|_key| transport)
                .expect("initializing swarm");

            builder
                .with_behaviour(|_| behaviour)
                .expect("initializing swarm")
                .with_swarm_config(|_| config)
                .build()
        };

        let mut network = Network {
            peer_id: PeerId::from_public_key(&PublicKey::from(local_key.public().clone())),
            swarm,
            subscribed_topics: Arc::new(Mutex::new(HashSet::new())),
        };

        network.start_network_worker(config).await?;

        Ok(network)
    }

    async fn start_network_worker(&mut self, config: &NetworkConfig) -> anyhow::Result<()> {
        info!("Libp2p starting .... ");

        let mut multi_addr: Multiaddr = config.socket_address.into();
        multi_addr.push(Protocol::Tcp(config.socket_port));

        match self.swarm.listen_on(multi_addr.clone()) {
            Ok(listener_id) => {
                info!(
                    "Listening on {:?} with peer_id {:?} {listener_id:?}",
                    multi_addr, self.peer_id
                );
            }
            Err(err) => {
                error!("Failed to start libp2p peer listen on {multi_addr:?}, error: {err:?}",);
            }
        }

        for bootnode in &config.discv5_config.bootnodes {
            if let (Some(ipv4), Some(tcp_port)) = (bootnode.ip4(), bootnode.tcp4()) {
                let mut multi_addr = Multiaddr::empty();
                multi_addr.push(ipv4.into());
                multi_addr.push(Protocol::Tcp(tcp_port));
                self.swarm.dial(multi_addr).unwrap();
            }
        }

        for topic in &config.gossipsub_config.topics {
            if self.subscribe_to_topic(*topic) {
                info!("Subscribed to topic: {topic}");
            } else {
                error!("Failed to subscribe to topic: {topic}");
            }
        }

        Ok(())
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    pub fn enr(&self) -> Enr {
        self.swarm.behaviour().discovery.local_enr().clone()
    }

    /// polling the libp2p swarm for network events.
    pub async fn polling_events(&mut self) -> ReamNetworkEvent {
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => {
                    if let Some(event) = self.parse_swarm_event(event){
                        return event;
                    }
                }
            }
        }
    }

    fn parse_swarm_event(
        &mut self,
        event: SwarmEvent<ReamBehaviourEvent>,
    ) -> Option<ReamNetworkEvent> {
        // currently no-op for any network events
        info!("Event: {:?}", event);
        match event {
            SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
                ReamBehaviourEvent::Identify(_) => None,
                ReamBehaviourEvent::Discovery(DiscoveredPeers { peers }) => {
                    self.handle_discovered_peers(peers);
                    None
                }
                ReamBehaviourEvent::ReqResp(message) => {
                    info!("Unhandled reqresp event {message:?}");
                    None
                }
                ReamBehaviourEvent::Gossipsub(event) => {
                    self.handle_gossipsub_event(event);
                    None
                }
                ream_behavior_event => {
                    info!("Unhandled behaviour event: {ream_behavior_event:?}");
                    None
                }
            },
            swarm_event => {
                info!("Unhandled swarm event: {swarm_event:?}");
                None
            }
        }
    }

    fn handle_discovered_peers(&mut self, peers: HashMap<Enr, Option<Instant>>) {
        info!("Discovered peers: {:?}", peers);
        for (enr, _) in peers {
            let mut multiaddrs: Vec<Multiaddr> = Vec::new();
            if let Some(ip) = enr.ip4() {
                if let Some(tcp) = enr.tcp4() {
                    let mut multiaddr: Multiaddr = ip.into();
                    multiaddr.push(Protocol::Tcp(tcp));
                    multiaddrs.push(multiaddr);
                }
            }
            if let Some(ip6) = enr.ip6() {
                if let Some(tcp6) = enr.tcp6() {
                    let mut multiaddr: Multiaddr = ip6.into();
                    multiaddr.push(Protocol::Tcp(tcp6));
                    multiaddrs.push(multiaddr);
                }
            }
            for multiaddr in multiaddrs {
                if let Err(err) = self.swarm.dial(multiaddr) {
                    warn!("Failed to dial peer: {err:?}");
                }
            }
        }
    }

    fn handle_gossipsub_event(&mut self, event: GossipsubEvent) {
        info!("Gossipsub event: {:?}", event);
        match event {
            GossipsubEvent::Message {
                propagation_source,
                message_id: _,
                message,
            } => {
                trace!("Peer {propagation_source} sent gossipsub message: {message:?}");
            }
            GossipsubEvent::Subscribed { peer_id, topic } => {
                trace!("Peer {peer_id} subscribed to topic: {topic:?}");
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                trace!("Peer {peer_id} unsubscribed from topic: {topic:?}");
            }
            _ => {}
        }
    }

    fn subscribe_to_topic(&mut self, topic: GossipTopic) -> bool {
        self.subscribed_topics.lock().insert(topic);

        let topic: Topic = topic.into();

        self.swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&topic)
            .is_ok()
    }

    #[allow(dead_code)]
    fn unsubscribe_from_topic(&mut self, topic: GossipTopic) -> bool {
        self.subscribed_topics.lock().remove(&topic);

        let topic: Topic = topic.into();

        self.swarm.behaviour_mut().gossipsub.unsubscribe(&topic)
    }
}

pub fn build_transport(local_private_key: Keypair) -> io::Result<Boxed<(PeerId, StreamMuxerBox)>> {
    // mplex config
    let mut mplex_config = MplexConfig::new();
    mplex_config.set_max_buffer_size(256);
    mplex_config.set_max_buffer_behaviour(MaxBufferBehaviour::Block);

    let yamux_config = YamuxConfig::default();

    let tcp = TcpTransport::new(TcpConfig::default().nodelay(true))
        .upgrade(Version::V1)
        .authenticate(NoiseConfig::new(&local_private_key).expect("Noise disabled"))
        .multiplex(SelectUpgrade::new(yamux_config, mplex_config))
        .timeout(Duration::from_secs(10));
    let transport = tcp.boxed();

    let transport = DnsTransport::system(transport)?.boxed();

    Ok(transport)
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use alloy_primitives::aliases::B32;
    use ream_discv5::{config::DiscoveryConfig, subnet::Subnets};
    use ream_executor::ReamExecutor;
    use ream_network_spec::networks::{DEV, set_network_spec};
    use tokio::runtime::Runtime;

    use super::*;
    use crate::{
        config::NetworkConfig,
        gossipsub::{configurations::GossipsubConfig, topics::GossipTopicKind},
    };

    async fn create_network(
        socket_address: IpAddr,
        socket_port: u16,
        discovery_port: u16,
        bootnodes: Vec<Enr>,
        disable_discovery: bool,
        topics: Vec<GossipTopic>,
    ) -> anyhow::Result<Network> {
        let executor = ReamExecutor::new().unwrap();

        let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
            socket_address,
            discovery_port,
        ))
        .build();

        let config = NetworkConfig {
            socket_address,
            socket_port,
            discv5_config: DiscoveryConfig {
                discv5_config,
                bootnodes,
                socket_address,
                socket_port,
                discovery_port,
                disable_discovery,
                subnets: Subnets::new(),
            },
            gossipsub_config: GossipsubConfig {
                topics,
                ..Default::default()
            },
        };

        Network::init(executor, &config).await
    }

    #[test]
    fn test_p2p_gossipsub() {
        set_network_spec(DEV.clone());

        let runtime = Runtime::new().unwrap();

        let gossip_topics = vec![GossipTopic {
            fork: B32::ZERO,
            kind: GossipTopicKind::BeaconBlock,
        }];

        let mut network1 = runtime
            .block_on(create_network(
                "127.0.0.1".parse::<IpAddr>().unwrap(),
                9000,
                9001,
                vec![],
                true,
                gossip_topics.clone(),
            ))
            .unwrap();
        let network1_enr = network1.enr();
        let mut network2 = runtime
            .block_on(create_network(
                "127.0.0.1".parse::<IpAddr>().unwrap(),
                9002,
                9003,
                vec![network1_enr],
                false,
                gossip_topics.clone(),
            ))
            .unwrap();

        runtime.block_on(async {
            let network1_future = async {
                while let Some(event) = network1.swarm.next().await {
                    if let SwarmEvent::Behaviour(ReamBehaviourEvent::Gossipsub(
                        GossipsubEvent::Subscribed { peer_id: _, topic },
                    )) = &event
                    {
                        let _ = network1
                            .swarm
                            .behaviour_mut()
                            .gossipsub
                            .publish(topic.clone(), vec![]);
                    }
                    let _ = network1.parse_swarm_event(event);
                }
            };

            let network2_future = async {
                while let Some(event) = network2.swarm.next().await {
                    if let SwarmEvent::Behaviour(ReamBehaviourEvent::Gossipsub(
                        GossipsubEvent::Message { .. },
                    )) = &event
                    {
                        break;
                    }
                    let _ = network2.parse_swarm_event(event);
                }
            };

            tokio::select! {
                _ = network1_future => {}
                _ = network2_future => {}
            }
        });
    }
}
