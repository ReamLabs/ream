use std::{
    collections::HashMap,
    num::{NonZeroU8, NonZeroUsize},
    sync::Arc,
    time::Instant,
};

use anyhow::anyhow;
use discv5::{Enr, multiaddr::Protocol};
use enr::CombinedPublicKey;
use futures::StreamExt;
use libp2p::{
    Multiaddr, SwarmBuilder,
    connection_limits::{self, ConnectionLimits},
    gossipsub::MessageAuthenticity,
    identify,
    swarm::{Config, NetworkBehaviour, Swarm, SwarmEvent},
};
use libp2p_identity::{Keypair, PeerId, secp256k1::PublicKey as Secp256k1PublicKey};
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
}

/// NetworkService is responsible for the following:
/// 1. Peer management. (We will connect with static peers for PQ devnet.)
/// 2. Gossiping blocks and votes.
///
/// TBD: It will be best if we reuse the existing NetworkManagerService for the beacon node.
pub struct LeanNetworkService {
    lean_chain: Arc<RwLock<LeanChain>>,
    swarm: Swarm<ReamBehaviour>,
    peer_table: RwLock<Vec<PeerId>>,
}

impl LeanNetworkService {
    pub async fn new(
        config: Arc<LeanNetworkConfig>,
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
                SnappyTransform::new(config.gossipsub_config.config.max_transmit_size());
            GossipsubBehaviour::new_with_transform(
                MessageAuthenticity::Anonymous,
                config.gossipsub_config.config.clone(),
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
        let peer_table = RwLock::new(Vec::new());

        Ok(LeanNetworkService {
            lean_chain,
            swarm,
            peer_table,
        })
    }

    pub async fn start(mut self, peer_config: Bootnodes) -> anyhow::Result<()> {
        info!("LeanNetworkService started");
        info!(
            "Current LeanChain head: {}",
            self.lean_chain.read().await.head
        );

        let initial_peers = match peer_config {
            Bootnodes::Default => Bootnodes::get_static_lean_peers(),
            Bootnodes::None => vec![],
            Bootnodes::Custom(enrs) => [enrs, Bootnodes::get_static_lean_peers()].concat(),
        };
        let mut peers = HashMap::new();
        for peer in initial_peers {
            peers.insert(peer, None);
        }

        self.connect_to_peers(peers).await;
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
            SwarmEvent::Behaviour(_) => match None {
                Some(ReamBehaviourEvent::Identify(_)) => None,
                _ => None,
            },
            _ => None,
        }
    }

    async fn connect_to_peers(&mut self, peers: HashMap<Enr, Option<Instant>>) {
        trace!("Discovered peers: {peers:?}");
        for (enr, _) in peers {
            let mut multiaddrs: Vec<Multiaddr> = Vec::new();
            if let Some(ip) = enr.ip4()
                && let Some(tcp) = enr.tcp4()
            {
                let mut multiaddr: Multiaddr = ip.into();
                multiaddr.push(Protocol::Tcp(tcp));
                multiaddrs.push(multiaddr);
            }
            if let Some(ip6) = enr.ip6()
                && let Some(tcp6) = enr.tcp6()
            {
                let mut multiaddr: Multiaddr = ip6.into();
                multiaddr.push(Protocol::Tcp(tcp6));
                multiaddrs.push(multiaddr);
            }

            let mut successfully_dialed = false;
            for multiaddr in multiaddrs {
                if let Err(err) = self.swarm.dial(multiaddr) {
                    warn!("Failed to dial peer: {err:?}");
                } else {
                    successfully_dialed = true;
                }
            }

            if !successfully_dialed {
                trace!("Failed to dial any multiaddr for peer: {:?}", enr);
                continue;
            }

            if let Some(peer_id) = peer_id_from_enr(&enr) {
                info!("Connected to peer: {peer_id:?}",);
                self.peer_table.write().await.push(peer_id);
            }
        }
    }
}

pub fn peer_id_from_enr(enr: &Enr) -> Option<PeerId> {
    match enr.public_key() {
        CombinedPublicKey::Secp256k1(public_key) => {
            let encoded_public_key = public_key.to_encoded_point(true);
            let public_key = Secp256k1PublicKey::try_from_bytes(encoded_public_key.as_bytes())
                .ok()?
                .into();
            Some(PeerId::from_public_key(&public_key))
        }
        _ => None,
    }
}
