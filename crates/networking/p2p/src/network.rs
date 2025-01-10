use std::{
    fmt::Debug,
    num::{NonZeroU8, NonZeroUsize},
    pin::Pin,
    time::Duration,
};

use libp2p::{
    core::{muxing::StreamMuxerBox, transport::Boxed},
    futures::StreamExt,
    identify,
    multiaddr::Protocol,
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    yamux, Multiaddr, PeerId, Swarm, SwarmBuilder, Transport,
};
use libp2p_identity::{secp256k1, Keypair, PublicKey};
use ream_discv5::{config::NetworkConfig, discovery::Discovery};
use ream_executor::ReamExecutor;

#[derive(NetworkBehaviour)]
pub(crate) struct ReamBehaviour {
    pub identify: identify::Behaviour,

    pub discovery: Discovery,

    pub connection_registry: libp2p::connection_limits::Behaviour,
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
}

struct Executor(ReamExecutor);

impl libp2p::swarm::Executor for Executor {
    fn exec(&self, f: Pin<Box<dyn futures::Future<Output = ()> + Send>>) {
        self.0.spawn(f);
    }
}

impl Network {
    pub async fn init(executor: ReamExecutor, config: &NetworkConfig) -> Result<Self, String> {
        let local_key = secp256k1::Keypair::generate();

        let discovery = {
            let mut discovery = Discovery::new(Keypair::from(local_key.clone()), config).await?;
            discovery.discover_peers(16);
            discovery
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
                identify,
                connection_registry: connection_limits,
            }
        };

        let transport = build_transport(Keypair::from(local_key.clone()))
            .map_err(|e| format!("Failed to build transport: {:?}", e))?;

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
        };

        network.start_network_worker(config).await?;

        Ok(network)
    }

    async fn start_network_worker(&mut self, _config: &NetworkConfig) -> Result<(), String> {
        println!("Libp2p starting .... ");

        let mut multi_addr: Multiaddr = "/ip4/127.0.0.1".parse().unwrap();
        multi_addr.push(Protocol::Tcp(10000));

        match self.swarm.listen_on(multi_addr.clone()) {
            Ok(_) => {
                println!(
                    "Listening on {:?} with peer_id {:?}",
                    multi_addr, self.peer_id
                );
            }
            Err(_) => {
                println!("Failed to start libp2p peer listen on {:?}", multi_addr);
            }
        }

        Ok(())
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
        match event {
            SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
                ReamBehaviourEvent::Identify(_) => None,
                ReamBehaviourEvent::Discovery(_) => None,
                _ => None,
            },
            _ => None,
        }
    }
}

type BoxedTransport = Boxed<(PeerId, StreamMuxerBox)>;
pub fn build_transport(local_private_key: Keypair) -> std::io::Result<BoxedTransport> {
    // mplex config
    let mut mplex_config = libp2p_mplex::MplexConfig::new();
    mplex_config.set_max_buffer_size(256);
    mplex_config.set_max_buffer_behaviour(libp2p_mplex::MaxBufferBehaviour::Block);

    let yamux_config = yamux::Config::default();

    let tcp = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default().nodelay(true))
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(noise::Config::new(&local_private_key).expect("Noise disabled"))
        .multiplex(libp2p::core::upgrade::SelectUpgrade::new(
            yamux_config,
            mplex_config,
        ))
        .timeout(Duration::from_secs(10));
    let transport = tcp.boxed();

    let transport = libp2p::dns::tokio::Transport::system(transport)?.boxed();

    Ok(transport)
}
