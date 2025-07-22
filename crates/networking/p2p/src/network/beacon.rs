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
use delay_map::{HashMapDelay, HashSetDelay};
use discv5::{Enr, enr::CombinedPublicKey};
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder, Transport,
    connection_limits::{self, ConnectionLimits},
    core::{
        ConnectedPoint,
        muxing::StreamMuxerBox,
        transport::Boxed,
        upgrade::{SelectUpgrade, Version},
    },
    dns::Transport as DnsTransport,
    futures::StreamExt,
    gossipsub::{Event as GossipsubEvent, IdentTopic as Topic, Message, MessageAuthenticity},
    identify,
    multiaddr::Protocol,
    noise::Config as NoiseConfig,
    swarm::{self, ConnectionId, NetworkBehaviour, SwarmEvent},
    tcp::{Config as TcpConfig, tokio::Transport as TcpTransport},
    yamux,
};
use libp2p_identity::{Keypair, PublicKey, secp256k1, secp256k1::PublicKey as Secp256k1PublicKey};
use libp2p_mplex::{MaxBufferBehaviour, MplexConfig};
use parking_lot::{Mutex, RwLock};
use ream_consensus_misc::constants::genesis_validators_root;
use ream_discv5::discovery::{Discovery, DiscoveryOutEvent, QueryType};
use ream_executor::ReamExecutor;
use ream_network_spec::networks::network_spec;
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    time::interval,
};
use tracing::{error, info, trace, warn};
use yamux::Config as YamuxConfig;

use crate::{
    channel::{P2PCallbackResponse, P2PMessage, P2PRequest, P2PResponse},
    config::NetworkConfig,
    constants::{PING_INTERVAL_DURATION, TARGET_PEER_COUNT},
    gossipsub::{GossipsubBehaviour, snappy::SnappyTransform, topics::GossipTopic},
    network_state::NetworkState,
    peer::{CachedPeer, ConnectionState, Direction},
    req_resp::{
        ReqResp, ReqRespMessage,
        configurations::REQUEST_TIMEOUT,
        handler::{ReqRespMessageError, ReqRespMessageReceived, RespMessage},
        messages::{
            RequestMessage, ResponseMessage,
            beacon_blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request},
            blob_sidecars::BlobSidecarsByRootV1Request,
            meta_data::GetMetaDataV2,
            ping::Ping,
            status::Status,
        },
    },
    utils::read_meta_data_from_disk,
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
    DisconnectPeer(PeerId),
    RequestMessage {
        peer_id: PeerId,
        stream_id: u64,
        connection_id: ConnectionId,
        message: RequestMessage,
    },
    GossipsubMessage {
        message: Message,
    },
}

struct Executor(ReamExecutor);

impl libp2p::swarm::Executor for Executor {
    fn exec(&self, f: Pin<Box<dyn futures::Future<Output = ()> + Send>>) {
        self.0.spawn(f);
    }
}

pub struct Network {
    peer_id: PeerId,
    swarm: Swarm<ReamBehaviour>,
    subscribed_topics: Arc<Mutex<HashSet<GossipTopic>>>,
    callbacks: HashMapDelay<u64, mpsc::Sender<anyhow::Result<P2PCallbackResponse>>>,
    request_id: u64,
    network_state: Arc<NetworkState>,
    peers_to_ping: HashSetDelay<PeerId>,
}

impl Network {
    /// Initializes the network by:
    /// - Creating a local keypair
    /// - Setting up the discovery, req_resp and gossipsub behaviours
    /// - Starting P2P listening and discovery
    /// - Connecting to the configured bootnodes
    /// - Subscribing to the configured gossipsub topics
    ///
    /// Note that this function starts P2P listening, but not handling network events yet.
    /// Event handling starts when `Network::start()` is called.
    pub async fn init(
        executor: ReamExecutor,
        config: &NetworkConfig,
        status: Status,
    ) -> anyhow::Result<Self> {
        let local_key = secp256k1::Keypair::generate();

        let mut discovery =
            Discovery::new(Keypair::from(local_key.clone()), &config.discv5_config).await?;
        discovery.discover_peers(QueryType::Peers, 16);

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
            let limits = ConnectionLimits::default()
                .with_max_pending_incoming(Some(5))
                .with_max_pending_outgoing(Some(16))
                .with_max_established_per_peer(Some(1));

            connection_limits::Behaviour::new(limits)
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

        let local_enr = discovery.local_enr();
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
            let config = swarm::Config::with_executor(Executor(executor))
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

        let network_state = Arc::new(NetworkState {
            local_enr: RwLock::new(local_enr),
            peer_table: RwLock::new(HashMap::new()),
            meta_data: RwLock::new(
                read_meta_data_from_disk(config.data_dir.clone()).unwrap_or_else(|err| {
                    error!("Failed to read meta data from disk: {err:?}");
                    GetMetaDataV2::default()
                }),
            ),
            status: RwLock::new(status),
            data_dir: config.data_dir.clone(),
        });

        let mut network = Network {
            peer_id: PeerId::from_public_key(&PublicKey::from(local_key.public().clone())),
            swarm,
            subscribed_topics: Arc::new(Mutex::new(HashSet::new())),
            callbacks: HashMapDelay::new(REQUEST_TIMEOUT),
            request_id: 0,
            network_state,
            peers_to_ping: HashSetDelay::new(PING_INTERVAL_DURATION),
        };

        network.start_network_worker(config).await?;

        Ok(network)
    }

    async fn start_network_worker(&mut self, config: &NetworkConfig) -> anyhow::Result<()> {
        info!("Libp2p starting .... ");

        let mut multi_addr: Multiaddr = config.discv5_config.socket_address.into();
        multi_addr.push(Protocol::Tcp(config.discv5_config.socket_port));

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

        let mut bootnodes = HashMap::new();
        for bootnode in config.discv5_config.bootnodes.clone() {
            bootnodes.insert(bootnode, None);
        }
        self.handle_discovered_peers(bootnodes);

        for topic in &config.gossipsub_config.topics {
            if self.subscribe_to_topic(*topic) {
                info!("Subscribed to topic: {topic}");
            } else {
                error!("Failed to subscribe to topic: {topic}");
            }
        }

        Ok(())
    }

    /// Returns the local node's peer id.
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Returns the local node's ENR.
    pub fn enr(&self) -> Enr {
        self.network_state.local_enr.read().clone()
    }

    fn request_id(&mut self) -> u64 {
        let request_id = self.request_id;
        self.request_id += 1;
        request_id
    }

    /// Returns the local node's network state such as peer table.
    pub fn network_state(&self) -> Arc<NetworkState> {
        self.network_state.clone()
    }

    /// Returns the cached peer from the peer table.
    pub fn cached_peer(&self, id: &PeerId) -> Option<CachedPeer> {
        self.network_state.peer_table.read().get(id).cloned()
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

    /// Starts monitoring for network events. The network worker awaits for different types
    /// of network events:
    /// - A swarm event
    /// - A p2p message
    /// - A peer pinging
    /// - An interval tick to perform p2p maintenance e.g. peer pinging, peer clean up and peer
    ///   discovery
    ///
    /// The network worker will then route each event to the appropriate handler. The handlers are
    /// defined in `NetworkManagerService`.
    pub async fn start(
        mut self,
        manager_sender: UnboundedSender<ReamNetworkEvent>,
        mut p2p_receiver: UnboundedReceiver<P2PMessage>,
    ) {
        let mut status_interval = interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => {
                    if let Some(event) = self.parse_swarm_event(event).await {
                        if let Err(err) = manager_sender.send(event) {
                            warn!("Failed to send event: {err:?}");
                        }
                    }
                }
                Some(event) = p2p_receiver.recv() => {
                    match event {
                        P2PMessage::Request(request) => match request {
                            P2PRequest::BlockRange { peer_id, start, count, callback } => {
                                if let Some(request_id) = self.send_request(peer_id, RequestMessage::BeaconBlocksByRange(BeaconBlocksByRangeV2Request::new(start, count))) {
                                    self.callbacks.insert(request_id, callback);
                                } else if let Err(err) = callback.send(Ok(P2PCallbackResponse::Disconnected)).await {
                                    warn!("Failed to send error response: {err:?}");
                                }
                            },
                            P2PRequest::BlockRoots { peer_id, roots, callback } => {
                                if let Some(request_id) = self.send_request(peer_id, RequestMessage::BeaconBlocksByRoot(BeaconBlocksByRootV2Request::new(roots))) {
                                    self.callbacks.insert(request_id, callback);
                                } else if let Err(err) = callback.send(Ok(P2PCallbackResponse::Disconnected)).await {
                                    warn!("Failed to send error response: {err:?}");
                                }
                            },
                            P2PRequest::BlobIdentifiers { peer_id, blob_identifiers, callback } => {
                                if let Some(request_id) = self.send_request(peer_id, RequestMessage::BlobSidecarsByRoot(BlobSidecarsByRootV1Request::new(blob_identifiers))) {
                                    self.callbacks.insert(request_id, callback);
                                } else if let Err(err) = callback.send(Ok(P2PCallbackResponse::Disconnected)).await {
                                    warn!("Failed to send error response: {err:?}");
                                }
                            }
                            P2PRequest::Status { peer_id, status } => {
                                self.send_request(peer_id, RequestMessage::Status(status));
                            }
                        },
                        P2PMessage::Response(P2PResponse {peer_id, connection_id, stream_id, message}) => {
                            self.swarm.behaviour_mut().req_resp.send_response(peer_id, connection_id, stream_id, *message)
                        },
                        P2PMessage::Gossip(message) => {
                            if let Err(err) = self.swarm.behaviour_mut().gossipsub.publish(message.topic, message.data) {
                                warn!("Failed to publish gossip message: {err}");
                            }
                        }
                    }
                }
                Some(Ok(peer_id)) = self.peers_to_ping.next() => {
                    if self.network_state.peer_table.read().get(&peer_id).is_none() {
                        warn!("Peer {peer_id} is not connected, skipping ping");
                        continue;
                    }

                    let ping_message = RequestMessage::Ping(Ping::new(self.network_state.meta_data.read().seq_number));
                    self.send_request(peer_id, ping_message);

                    self.peers_to_ping.insert(peer_id);
                }
                Some(Ok((_, callback))) = self.callbacks.next() => {
                    if let Err(err) = callback.send(Ok(P2PCallbackResponse::Timeout)).await {
                        warn!("Failed to send timeout response: {err:?}");
                    }
                }
                _ = status_interval.tick() => {
                    let now = Instant::now();
                    let mut peer_table = self.network_state.peer_table.write();

                    // Clean up stale peers
                    peer_table.retain(|_, peer| now.duration_since(peer.last_seen) < Duration::from_secs(360));

                    // Compute peer state counts, status/meta counts in a single pass
                    let mut counts: HashMap<ConnectionState, usize> = HashMap::new();
                    let mut status_is_some_count = 0;
                    let mut meta_data_some_count = 0;

                    for peer in peer_table.values() {
                        *counts.entry(peer.state).or_insert(0) += 1;
                        if peer.status.is_some() {
                            status_is_some_count += 1;
                        }
                        if peer.meta_data.is_some() {
                            meta_data_some_count += 1;
                        }
                    }

                    let peer_count = peer_table.len();
                    let peers_to_ping_count = self.peers_to_ping.len();
                    let seq_number = self.network_state.meta_data.read().seq_number;

                    info!("Peer statuses: {counts:?}, Peers with Status {status_is_some_count}, Peers with MetaData {meta_data_some_count}, Peers to ping: {peers_to_ping_count}, MetaData seq_number: {seq_number}");

                    if peer_count < TARGET_PEER_COUNT {
                        info!("Peer count is below target: {peer_count}, discovering more peers");
                        self.swarm
                            .behaviour_mut()
                            .discovery
                            .discover_peers(QueryType::Peers, 16);
                    }
                }
            }
        }
    }

    fn send_request(&mut self, peer_id: PeerId, message: RequestMessage) -> Option<u64> {
        if !self.swarm.is_connected(&peer_id) {
            return None;
        }

        let request_id = self.request_id();
        self.swarm
            .behaviour_mut()
            .req_resp
            .send_request(peer_id, request_id, message);

        Some(request_id)
    }

    async fn parse_swarm_event(
        &mut self,
        event: SwarmEvent<ReamBehaviourEvent>,
    ) -> Option<ReamNetworkEvent> {
        match event {
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                ..
            } => {
                self.network_state.upsert_peer(
                    peer_id,
                    None,
                    ConnectionState::Disconnected,
                    Direction::Outbound,
                    None,
                );
                None
            }
            // We only handle this for incoming connections
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                if let ConnectedPoint::Listener { send_back_addr, .. } = &endpoint {
                    self.network_state.upsert_peer(
                        peer_id,
                        Some(send_back_addr.clone()),
                        ConnectionState::Connecting,
                        Direction::Inbound,
                        None,
                    );
                } else {
                    // send status request to the peer
                    let status_message =
                        RequestMessage::Status(self.network_state.status.read().clone());
                    self.send_request(peer_id, status_message);
                    let ping_message = RequestMessage::Ping(Ping::new(
                        self.network_state.meta_data.read().seq_number,
                    ));
                    self.send_request(peer_id, ping_message);
                    self.peers_to_ping.insert(peer_id);
                }

                None
            }
            SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
                ReamBehaviourEvent::Identify(_) => None,
                ReamBehaviourEvent::Discovery(discovery_event) => match discovery_event {
                    DiscoveryOutEvent::DiscoveredPeers { peers } => {
                        self.handle_discovered_peers(peers);
                        None
                    }
                    DiscoveryOutEvent::UpdatedEnr { enr } => {
                        *self.network_state.local_enr.write() = enr;
                        None
                    }
                },
                ReamBehaviourEvent::ReqResp(message) => {
                    self.handle_request_response_event(message).await
                }
                ReamBehaviourEvent::Gossipsub(event) => self.handle_gossipsub_event(event),
                ream_behavior_event => {
                    info!("Unhandled behaviour event: {ream_behavior_event:?}");
                    None
                }
            },
            swarm_event => {
                trace!("Unhandled swarm event: {swarm_event:?}");
                None
            }
        }
    }

    fn handle_discovered_peers(&mut self, peers: HashMap<Enr, Option<Instant>>) {
        trace!("Discovered peers: {peers:?}");
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

            if let Some(peer_id) = Network::peer_id_from_enr(&enr) {
                self.network_state.upsert_peer(
                    peer_id,
                    None,
                    ConnectionState::Connecting,
                    Direction::Outbound,
                    Some(enr.clone()),
                );
                self.peers_to_ping.insert_at(peer_id, Duration::ZERO);
            }
        }
    }

    async fn handle_request_response_event(
        &mut self,
        message: ReqRespMessage,
    ) -> Option<ReamNetworkEvent> {
        let ReqRespMessage {
            peer_id,
            connection_id,
            message,
        } = message;

        // update last seen time for the peer
        self.network_state
            .peer_table
            .write()
            .entry(peer_id)
            .and_modify(|cached_peer| {
                cached_peer.update_last_seen();
            });

        let message = match message {
            Ok(message) => message,
            Err(err) => {
                if let ReqRespMessageError::Outbound { request_id, .. } = &err {
                    if let Some(callback) = self.callbacks.get(request_id) {
                        if let Err(err) = callback.send(Err(anyhow!("{err:?}"))).await {
                            warn!("Failed to send error response: {err:?}");
                        }
                    }
                }
                return None;
            }
        };

        match message {
            ReqRespMessageReceived::Request { stream_id, message } => match *message {
                RequestMessage::MetaData(get_meta_data_v2) => {
                    trace!(
                        ?peer_id,
                        ?stream_id,
                        ?connection_id,
                        ?get_meta_data_v2,
                        "Received GetMetaDataV2 request"
                    );
                    self.swarm.behaviour_mut().req_resp.send_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        RespMessage::Response(Box::new(ResponseMessage::MetaData(
                            self.network_state.meta_data.read().clone().into(),
                        ))),
                    );
                    None
                }
                RequestMessage::Ping(ping) => {
                    trace!(
                        ?peer_id,
                        ?stream_id,
                        ?connection_id,
                        ?ping,
                        "Received Ping request"
                    );
                    self.swarm.behaviour_mut().req_resp.send_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        RespMessage::Response(Box::new(ResponseMessage::Ping(Ping::new(
                            self.network_state.meta_data.read().seq_number,
                        )))),
                    );
                    None
                }
                RequestMessage::Goodbye(goodbye) => {
                    trace!(
                        ?peer_id,
                        ?stream_id,
                        ?connection_id,
                        ?goodbye,
                        "Received Goodbye message"
                    );
                    None
                }
                RequestMessage::Status(status) => {
                    trace!(
                        ?peer_id,
                        ?stream_id,
                        ?connection_id,
                        ?status,
                        "Received Status request"
                    );

                    self.handle_status_req_resp_event(peer_id, status.clone());

                    Some(ReamNetworkEvent::RequestMessage {
                        peer_id,
                        stream_id,
                        connection_id,
                        message: RequestMessage::Status(status),
                    })
                }
                _ => Some(ReamNetworkEvent::RequestMessage {
                    peer_id,
                    stream_id,
                    connection_id,
                    message: *message,
                }),
            },
            ReqRespMessageReceived::Response {
                request_id,
                message,
            } => {
                match *message.clone() {
                    ResponseMessage::MetaData(meta_data) => {
                        trace!(
                            ?peer_id,
                            ?request_id,
                            "Received MetaData response: seq_number: {}",
                            meta_data.seq_number
                        );

                        self.network_state
                            .peer_table
                            .write()
                            .entry(peer_id)
                            .and_modify(|cached_peer| {
                                cached_peer.meta_data = Some(meta_data.as_ref().clone());
                            });
                    }
                    ResponseMessage::Ping(ping) => {
                        trace!(
                            ?peer_id,
                            ?request_id,
                            "Received Ping response: seq_number: {}",
                            ping.sequence_number
                        );

                        let cached_peer =
                            self.network_state.peer_table.read().get(&peer_id).cloned();
                        if let Some(cached_peer) = cached_peer {
                            if cached_peer.meta_data.is_none()
                                || ping.sequence_number
                                    != cached_peer
                                        .meta_data
                                        .as_ref()
                                        .map_or(0, |meta_data| meta_data.seq_number)
                            {
                                let meta_data_message = RequestMessage::MetaData(
                                    self.network_state.meta_data.read().clone().into(),
                                );
                                self.send_request(peer_id, meta_data_message);
                            }
                        }
                    }
                    ResponseMessage::Status(status) => {
                        trace!(
                            ?peer_id,
                            ?request_id,
                            "Received Status response: fork_digest: {}, head_slot: {}",
                            status.fork_digest,
                            status.head_slot
                        );

                        self.handle_status_req_resp_event(peer_id, status);
                    }
                    _ => {}
                }

                self.callbacks.update_timeout(&request_id, REQUEST_TIMEOUT);
                if let Some(callback) = self.callbacks.get(&request_id) {
                    if let Err(err) = callback
                        .send(Ok(P2PCallbackResponse::ResponseMessage(message)))
                        .await
                    {
                        warn!("Failed to send response: {err:?}");
                    }
                }

                None
            }
            ReqRespMessageReceived::EndOfStream { request_id } => {
                let callback = self.callbacks.remove(&request_id);
                if let Some(callback) = callback {
                    if let Err(err) = callback.send(Ok(P2PCallbackResponse::EndOfStream)).await {
                        warn!("Failed to send end of stream: {err:?}");
                    }
                }
                None
            }
        }
    }

    fn handle_status_req_resp_event(&mut self, peer_id: PeerId, status: Status) {
        if self.network_state.peer_table.read().get(&peer_id).is_some() {
            // We only want to have peers on the same network as us
            let fork_digest = network_spec().fork_digest(genesis_validators_root());
            if status.fork_digest != fork_digest {
                warn!(
                    "Peer {peer_id} is not on the same network as us, removing from peer table, fork_digest: {}, our fork_digest: {fork_digest}",
                    status.fork_digest,
                );
                self.network_state.peer_table.write().remove(&peer_id);
            } else {
                self.network_state
                    .peer_table
                    .write()
                    .entry(peer_id)
                    .and_modify(|cached_peer| {
                        cached_peer.state = ConnectionState::Connected;
                        cached_peer.status = Some(status);
                    });
            }
        }
    }

    fn handle_gossipsub_event(&mut self, event: GossipsubEvent) -> Option<ReamNetworkEvent> {
        match event {
            GossipsubEvent::Message {
                propagation_source: _,
                message_id: _,
                message,
            } => Some(ReamNetworkEvent::GossipsubMessage { message }),
            GossipsubEvent::Subscribed { peer_id, topic } => {
                trace!("Peer {peer_id} subscribed to topic: {topic:?}");
                None
            }
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                trace!("Peer {peer_id} unsubscribed from topic: {topic:?}");
                None
            }
            _ => None,
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
    use discv5::enr::CombinedKey;
    use k256::ecdsa::SigningKey;
    use libp2p_identity::{Keypair, PeerId};
    use ream_discv5::{
        config::DiscoveryConfig,
        subnet::{AttestationSubnets, SyncCommitteeSubnets},
    };
    use ream_executor::ReamExecutor;
    use ream_network_spec::networks::initialize_test_network_spec;
    use tokio::{runtime::Runtime, time::sleep};

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
            discv5_config: DiscoveryConfig {
                discv5_config,
                bootnodes,
                socket_address,
                socket_port,
                discovery_port,
                disable_discovery,
                attestation_subnets: AttestationSubnets::new(),
                sync_committee_subnets: SyncCommitteeSubnets::new(),
            },
            gossipsub_config: GossipsubConfig {
                topics,
                ..Default::default()
            },
            data_dir: std::env::temp_dir().join("ream_network_test"),
        };

        Network::init(
            executor,
            &config,
            Status {
                fork_digest: network_spec().fork_digest(genesis_validators_root()),
                ..Default::default()
            },
        )
        .await
    }

    #[test]
    fn peer_id_derived_from_enr_matches_libp2p() {
        let libp2p_keypair = Keypair::generate_secp256k1();
        let secret = libp2p_keypair
            .clone()
            .try_into_secp256k1()
            .unwrap()
            .secret()
            .to_bytes();
        let signing = SigningKey::from_slice(&secret).unwrap();

        let enr_key = CombinedKey::Secp256k1(signing);
        let enr = Enr::builder().build(&enr_key).unwrap();

        let expected = PeerId::from_public_key(&libp2p_keypair.public());
        let actual = Network::peer_id_from_enr(&enr).expect("peer id");

        assert_eq!(expected, actual);
    }

    #[test]
    fn insert_then_read_returns_snapshot() {
        initialize_test_network_spec();

        let tokio_runtime = Runtime::new().unwrap();

        let network = tokio_runtime.block_on(async {
            create_network("127.0.0.1".parse().unwrap(), 0, 0, vec![], true, vec![])
                .await
                .unwrap()
        });

        let peer_id = PeerId::random();
        let address: Multiaddr = "/ip4/1.2.3.4/tcp/9000".parse().unwrap();

        network.network_state.upsert_peer(
            peer_id,
            Some(address.clone()),
            ConnectionState::Connecting,
            Direction::Outbound,
            None,
        );

        let cached_peer_snapshot = network.cached_peer(&peer_id).expect("peer should exist");

        assert_eq!(cached_peer_snapshot.peer_id, peer_id);
        assert_eq!(cached_peer_snapshot.state, ConnectionState::Connecting);
        assert_eq!(cached_peer_snapshot.direction, Direction::Outbound);
        assert_eq!(cached_peer_snapshot.last_seen_p2p_address, Some(address));
        assert!(cached_peer_snapshot.enr.is_none());
    }

    #[test]
    fn update_existing_peer() {
        initialize_test_network_spec();

        let tokio_runtime = Runtime::new().unwrap();

        let network = tokio_runtime.block_on(async {
            create_network("127.0.0.1".parse().unwrap(), 0, 0, vec![], true, vec![])
                .await
                .unwrap()
        });

        let peer_id = PeerId::random();

        network.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connecting,
            Direction::Outbound,
            None,
        );

        network.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
            None,
        );

        let cached_peer_snapshot = network.cached_peer(&peer_id).expect("peer exists in cache");

        assert_eq!(cached_peer_snapshot.state, ConnectionState::Connected);
        assert_eq!(cached_peer_snapshot.direction, Direction::Outbound);
    }

    #[test]
    fn cached_peer_unknown_returns_none() {
        initialize_test_network_spec();

        let tokio_runtime = Runtime::new().unwrap();

        let network = tokio_runtime.block_on(async {
            create_network("127.0.0.1".parse().unwrap(), 0, 0, vec![], true, vec![])
                .await
                .unwrap()
        });

        let peer_id = PeerId::random();

        assert!(network.cached_peer(&peer_id).is_none());
    }

    #[test]
    fn test_p2p_gossipsub() {
        initialize_test_network_spec();

        let runtime = Runtime::new().unwrap();

        let gossip_topics = vec![GossipTopic {
            fork: B32::ZERO,
            kind: GossipTopicKind::BeaconBlock,
        }];

        let mut network_1 = runtime
            .block_on(create_network(
                "127.0.0.1".parse::<IpAddr>().unwrap(),
                9000,
                9001,
                vec![],
                true,
                gossip_topics.clone(),
            ))
            .unwrap();
        let network_1_enr = network_1.enr();
        let mut network_2 = runtime
            .block_on(create_network(
                "127.0.0.1".parse::<IpAddr>().unwrap(),
                9002,
                9003,
                vec![network_1_enr],
                false,
                gossip_topics.clone(),
            ))
            .unwrap();

        runtime.block_on(async {
            let network_1_future = async {
                while let Some(event) = network_1.swarm.next().await {
                    if let SwarmEvent::Behaviour(ReamBehaviourEvent::Gossipsub(
                        GossipsubEvent::Subscribed { peer_id: _, topic },
                    )) = &event
                    {
                        let _ = network_1
                            .swarm
                            .behaviour_mut()
                            .gossipsub
                            .publish(topic.clone(), vec![]);
                    }
                    let _ = network_1.parse_swarm_event(event).await;
                }
            };

            let network_2_future = async {
                while let Some(event) = network_2.swarm.next().await {
                    if let SwarmEvent::Behaviour(ReamBehaviourEvent::Gossipsub(
                        GossipsubEvent::Message { .. },
                    )) = &event
                    {
                        break;
                    }
                    let _ = network_2.parse_swarm_event(event).await;
                }
            };

            tokio::select! {
                _ = network_1_future => {}
                _ = network_2_future => {}
            }
        });
    }

    #[test]
    fn test_peer_table_lifecycle() {
        initialize_test_network_spec();

        let tokio_runtime = Runtime::new().unwrap();

        let mut network_1 = tokio_runtime
            .block_on(create_network(
                "127.0.0.1".parse().unwrap(),
                9300,
                9301,
                vec![],
                true,
                vec![],
            ))
            .unwrap();

        let mut network_2 = tokio_runtime
            .block_on(create_network(
                "127.0.0.1".parse().unwrap(),
                9302,
                9303,
                vec![],
                true,
                vec![],
            ))
            .unwrap();

        let peer_id_network_1 = network_1.peer_id();
        let peer_id_network_2 = network_2.peer_id();

        tokio_runtime.block_on(async {
            let peers = HashMap::from([(network_1.enr(), None)]);
            network_2.handle_discovered_peers(peers);

            let network_1_poll_task =  async   {
                while let Some(event) = network_1.swarm.next().await {
                    if let Some(ReamNetworkEvent::RequestMessage {
                        peer_id,
                        stream_id,
                        connection_id,
                        message: RequestMessage::Status(status),
                    }) = network_1.parse_swarm_event(event).await {
                                network_1
                                    .swarm
                                    .behaviour_mut()
                                    .req_resp
                                    .send_response(
                                        peer_id,
                                        connection_id,
                                        stream_id,
                                        RespMessage::Response(Box::new(ResponseMessage::Status(
                                            status,
                                        ))),
                                    );
                    }
                }};

            let network_2_poll_task =  async   {
                while let Some(event) = network_2.swarm.next().await {
                    network_2.parse_swarm_event(event).await;
                    if matches!(
                        network_2.cached_peer(&peer_id_network_1),
                        Some(peer) if peer.state == ConnectionState::Connected && peer.direction == Direction::Outbound
                    ) {
                        break;
                    }
                }
            };


            tokio::select! {
                _ = network_1_poll_task => {}
                _ = network_2_poll_task => {}
                _ = sleep(Duration::from_secs(10)) => {}
            }
        }
       );

        let peer_from_network_1 = network_1
            .cached_peer(&peer_id_network_2)
            .expect("network_1 peer exists");
        let peer_from_network_2 = network_2
            .cached_peer(&peer_id_network_1)
            .expect("network_2 peer exists");

        assert_eq!(peer_from_network_1.state, ConnectionState::Connected);
        assert_eq!(peer_from_network_1.direction, Direction::Inbound);

        assert_eq!(peer_from_network_2.state, ConnectionState::Connected);
        assert_eq!(peer_from_network_2.direction, Direction::Outbound);
    }
}
