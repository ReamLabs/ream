use std::{
    io::Error,
    num::{NonZeroU8, NonZeroUsize},
    pin::Pin,
    time::Duration,
};

use discv5::enr::CombinedKey;
use futures::future::Either;
use libp2p::{
    core::{
        muxing::StreamMuxerBox,
        transport::{Boxed, ListenerId},
    },
    futures::StreamExt,
    identify, identity,
    identity::Keypair,
    noise, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
    yamux, PeerId, Swarm, SwarmBuilder, Transport, TransportError,
};
use task_executor::TaskExecutor;

use crate::{config::NetworkConfig, discovery::Discovery};

#[derive(NetworkBehaviour)]
pub(crate) struct ReamBehaviour {
    pub identify: identify::Behaviour,

    pub discovery: Discovery,

    pub connection_registry: libp2p::connection_limits::ConnectionLimits,
}

// TODO: these are stub events which needs to be replaced
pub enum NetworkEvent {
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

impl Network {
    pub async fn init(
        executor: task_executor::TaskExecutor,
        config: &NetworkConfig,
    ) -> Result<Network, String> {
        let local_key = identity::Keypair::generate_ed25519();
        let key = local_key.try_into_secp256k1().expect("right key type");
        let secret = discv5::enr::k256::ecdsa::SigningKey::from_slice(&key.secret().to_bytes())
            .expect("libp2p key must be valid");
        let enr_local = CombinedKey::Secp256k1(secret);
        let enr = discv5::enr::Enr::builder().build(&enr_local).unwrap();
        let node_local_id = enr.node_id();

        let discovery = {
            let mut discovery = Discovery::new(key.into().clone(), &config).await?;
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

        let behaviour = {
            ReamBehaviour {
                discovery,
                identify,
                connection_registry: connection_limits,
            }
        };

        struct Executor(TaskExecutor);
        impl libp2p::swarm::Executor for Executor {
            fn exec(&self, f: Pin<Box<dyn futures::Future<Output = ()> + Send>>) {
                self.0.spawn(f);
            }
        }
        let transport = build_transport(key.clone(), true)
            .map_err(|e| format!("Failed to build transport: {:?}", e))?;

        let swarm = {
            let config = libp2p::swarm::Config::with_executor(Executor(executor))
                .with_notify_handler_buffer_size(NonZeroUsize::new(7).expect("Not zero"))
                .with_per_connection_event_buffer_size(4)
                .with_dial_concurrency_factor(NonZeroU8::new(1).unwrap());

            let builder = SwarmBuilder::with_existing_identity(key)
                .with_tokio()
                .with_other_transport(|_key| transport)
                .expect("infalible");

            builder
                .with_behaviour(|_| behaviour)
                .expect("infalible")
                .with_swarm_config(|_| config)
                .build()
        };

        let mut network = Network {
            peer_id: node_local_id,
            swarm,
        };

        network.start_network_worker(&config).await?;
        Ok(network)
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                message = self.swarm.select_next_some() => {

                }
            }
        }
    }

    async fn start_network_worker(&mut self, config: &NetworkConfig) -> Result<(), String> {
        println!("Libp2p starting .... ");

        for listen_multiaddr in config.listen_addresses.libp2p_addresses() {
            match self.swarm.listen_on(listen_multiaddr.clone()) {
                Ok(_) => {
                    println!("Listening on {:?}", listen_multiaddr);
                }
                Err(_) => {
                    println!(
                        "Failed to start libp2p peer listen on {:?}",
                        listen_multiaddr
                    );
                }
            }
        }

        Ok(())
    }

    /// polling the libp2p swarm for network events.
    pub async fn polling_events(&mut self) -> NetworkEvent {
        loop {
            tokio::select! {
                Some(event) = self.swarm.select_next_some() => {
                    if let Some(event) = self.parse_swarm_event(event){
                        return event;
                    }
                }
            }
        }
    }

    fn parse_swarm_event(&mut self, event: SwarmEvent<NetworkEvent>) -> Option<NetworkEvent> {
        // currently no-op for any network events
        match event {
            SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
                NetworkEvent::PeerConnectedIncoming(_) => None,
                NetworkEvent::PeerConnectedOutgoing(_) => None,
                NetworkEvent::PeerDisconnected(_) => None,
                NetworkEvent::Status(_) => None,
                NetworkEvent::Ping(_) => None,
                NetworkEvent::MetaData(_) => None,
                NetworkEvent::DisconnectPeer(_) => None,
                NetworkEvent::DiscoverPeers(_) => None,
            },
            SwarmEvent::ConnectionEstablished { .. } => None,
            SwarmEvent::ConnectionClosed { .. } => None,
            SwarmEvent::IncomingConnection { .. } => None,
            SwarmEvent::IncomingConnectionError { .. } => None,
            SwarmEvent::OutgoingConnectionError { .. } => None,
            SwarmEvent::NewListenAddr { .. } => None,
            SwarmEvent::ExpiredListenAddr { .. } => None,
            SwarmEvent::ListenerClosed { .. } => None,
            SwarmEvent::ListenerError { .. } => None,
            SwarmEvent::Dialing { .. } => None,
            SwarmEvent::NewExternalAddrCandidate { .. } => None,
            SwarmEvent::ExternalAddrConfirmed { .. } => None,
            SwarmEvent::ExternalAddrExpired { .. } => None,
            SwarmEvent::NewExternalAddrOfPeer { .. } => None,
            _ => None,
        }
    }
}

type BoxedTransport = Boxed<(PeerId, StreamMuxerBox)>;
pub fn build_transport(
    local_private_key: Keypair,
    quic_support: bool,
) -> std::io::Result<BoxedTransport> {
    // mplex config
    let mut mplex_config = libp2p_mplex::MplexConfig::new();
    mplex_config.set_max_buffer_size(256);
    mplex_config.set_max_buffer_behaviour(libp2p_mplex::MaxBufferBehaviour::Block);

    // yamux config
    let yamux_config = yamux::Config::default();
    // Creates the TCP transport layer
    let tcp = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default().nodelay(true))
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(
            noise::Config::new(&local_private_key)
                .expect("signing can fail only once during starting a node"),
        )
        .multiplex(libp2p::core::upgrade::SelectUpgrade::new(
            yamux_config,
            mplex_config,
        ))
        .timeout(Duration::from_secs(10));
    let transport = if quic_support {
        // Enables Quic
        // The default quic configuration suits us for now.
        let quic_config = libp2p::quic::Config::new(&local_private_key);
        let quic = libp2p::quic::tokio::Transport::new(quic_config);
        let transport = tcp
            .or_transport(quic)
            .map(|either_output, _| match either_output {
                Either::Left((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
                Either::Right((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
            });
        transport.boxed()
    } else {
        tcp.boxed()
    };

    // Enables DNS over the transport.
    let transport = libp2p::dns::tokio::Transport::system(transport)?.boxed();

    Ok(transport)
}
