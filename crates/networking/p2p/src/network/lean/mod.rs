use std::{
    collections::HashMap,
    fs,
    net::IpAddr,
    num::{NonZeroU8, NonZeroUsize},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use alloy_primitives::hex;
use anyhow::anyhow;
use delay_map::HashMapDelay;
use discv5::multiaddr::Protocol;
use futures::{StreamExt, stream::FuturesUnordered};
use libp2p::{
    Multiaddr, SwarmBuilder,
    connection_limits::{self, ConnectionLimits},
    core::ConnectedPoint,
    gossipsub::{Event as GossipsubEvent, IdentTopic, MessageAuthenticity, PublishError},
    identify,
    swarm::{Config, ConnectionId, NetworkBehaviour, Swarm, SwarmEvent},
};
use libp2p_identity::{Keypair, PeerId, secp256k1};
use ream_chain_lean::{
    messages::{LeanChainServiceMessage, RequestResult},
    p2p_request::{LeanP2PRequest, P2PCallbackRequest},
};
use ream_executor::ReamExecutor;
use ream_metrics::{
    LEAN_CONNECTION_EVENT_TOTAL, LEAN_DISCONNECTION_EVENT_TOTAL, LEAN_PEER_COUNT,
    inc_int_counter_vec, set_int_gauge_vec,
};
use ream_network_state_lean::{NetworkState, cached_peer::CachedPeer};
use ream_peer::{ConnectionState, Direction};
use ream_req_resp::{
    Chain, ReqResp, ReqRespMessage,
    handler::{ReqRespMessageReceived, RespMessage},
    lean::{
        NetworkEvent, ReamNetworkEvent, ResponseCallback,
        messages::{
            LeanRequestMessage, LeanResponseMessage, blocks::BlocksByRootV1Request, status::Status,
        },
    },
    messages::{RequestMessage, ResponseMessage},
};
use ssz::Encode;
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::{Duration, interval},
};
use tracing::{error, info, trace, warn};

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
};

const BOOTNODE_RETRY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(NetworkBehaviour)]
pub(crate) struct ReamBehaviour {
    pub identify: identify::Behaviour,

    /// The request-response domain
    pub req_resp: ReqResp,

    /// The gossip domain: gossipsub
    pub gossipsub: GossipsubBehaviour,

    pub connection_limits: connection_limits::Behaviour,
}

pub struct LeanNetworkConfig {
    pub gossipsub_config: LeanGossipsubConfig,
    pub socket_address: IpAddr,
    pub socket_port: u16,
    pub private_key_path: Option<std::path::PathBuf>,
}

pub struct LeanNetworkService {
    network_config: Arc<LeanNetworkConfig>,
    swarm: Swarm<ReamBehaviour>,
    chain_message_sender: UnboundedSender<LeanChainServiceMessage>,
    chain_callback_requests: HashMap<u64, (PeerId, mpsc::Sender<ResponseCallback>)>,
    outbound_p2p_request: UnboundedReceiver<LeanP2PRequest>,
    bootnode_retry_state: HashMapDelay<PeerId, (u32, Vec<Multiaddr>)>,
    request_id: AtomicU64,
    pub network_state: Arc<NetworkState>,
    check_canonical_futures: FuturesUnordered<oneshot::Receiver<(PeerId, bool)>>,
    pub multi_addr: Multiaddr,
}

impl LeanNetworkService {
    pub async fn new(
        network_config: Arc<LeanNetworkConfig>,
        executor: ReamExecutor,
        chain_message_sender: UnboundedSender<LeanChainServiceMessage>,
        outbound_p2p_request: UnboundedReceiver<LeanP2PRequest>,
        network_state: Arc<NetworkState>,
    ) -> anyhow::Result<Self> {
        let connection_limits = {
            let limits = ConnectionLimits::default()
                .with_max_pending_incoming(Some(5))
                .with_max_pending_outgoing(Some(16))
                .with_max_established_per_peer(Some(2));

            connection_limits::Behaviour::new(limits)
        };

        let local_key = if let Some(ref path) = network_config.private_key_path {
            let private_key_hex = fs::read_to_string(path).map_err(|err| {
                anyhow!("failed to read secret key file {}: {err}", path.display())
            })?;
            let private_key_bytes = hex::decode(private_key_hex.trim()).map_err(|err| {
                anyhow!(
                    "failed to decode hex from private key file {}: {err}",
                    path.display()
                )
            })?;
            let private_key =
                secp256k1::SecretKey::try_from_bytes(private_key_bytes).map_err(|err| {
                    anyhow!("failed to decode secp256k1 secret key from bytes: {err}")
                })?;

            Keypair::from(secp256k1::Keypair::from(private_key))
        } else {
            Keypair::generate_secp256k1()
        };

        let gossipsub = {
            let snappy_transform =
                SnappyTransform::new(network_config.gossipsub_config.config.max_transmit_size());
            GossipsubBehaviour::new_with_transform(
                MessageAuthenticity::Anonymous,
                network_config.gossipsub_config.config.clone(),
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
                .with_dial_concurrency_factor(NonZeroU8::new(1).expect("Not zero"));

            SwarmBuilder::with_existing_identity(local_key.clone())
                .with_tokio()
                .with_quic()
                .with_behaviour(|_| behaviour)?
                .with_swarm_config(|_| config)
                .build()
        };

        let mut multi_addr: Multiaddr = network_config.socket_address.into();
        multi_addr.push(Protocol::Udp(network_config.socket_port));
        multi_addr.push(Protocol::QuicV1);
        multi_addr.push(Protocol::P2p(local_key.public().to_peer_id()));
        info!("Listening on {multi_addr:?}");

        let mut lean_network_service = LeanNetworkService {
            network_config: network_config.clone(),
            swarm,
            chain_message_sender,
            outbound_p2p_request,
            bootnode_retry_state: HashMapDelay::new(BOOTNODE_RETRY_TIMEOUT),
            request_id: AtomicU64::new(1),
            network_state,
            check_canonical_futures: FuturesUnordered::new(),
            multi_addr: multi_addr.clone(),
            chain_callback_requests: HashMap::new(),
        };

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
        set_int_gauge_vec(&LEAN_PEER_COUNT, 0, &[]);

        let bootnode_addresses = bootnodes.to_multiaddrs_lean();
        let mut bootnode_redial_interval = interval(Duration::from_secs(20));
        let mut status_interval = interval(Duration::from_secs(4));

        loop {
            tokio::select! {
                _ = bootnode_redial_interval.tick() => {
                    self.connect_to_multinodes(bootnode_addresses.clone()).await;
                }
                Some(Ok((peer_id, (attempts, addresses)))) = self.bootnode_retry_state.next() => {
                    if matches!(self.network_state.peer_table.lock().get(&peer_id).map(|peer| peer.state), Some(ConnectionState::Connected)) {
                        continue;
                    }
                    if attempts >= 8 {
                        warn!("giving up on {peer_id:?} after 8 attempts");
                        continue;
                    };

                    for address in &addresses {
                        if let Err(err) = self.dial_peer(address.clone()) {
                            warn!("retry to peer_id: {peer_id:?}, address: {address} error: {err}");
                        }
                    }

                    self.bootnode_retry_state.insert(peer_id, (attempts + 1, addresses))
                }
                _ = status_interval.tick() => {
                    let mut peers_to_ping = Vec::new();

                    for cached_peer in self.network_state.peer_table.lock().values() {
                        if matches!(cached_peer.state, ConnectionState::Connected) {
                            let should_ping = if let Some(last_status_update) = cached_peer.last_status_update {
                                last_status_update.elapsed() > Duration::from_secs(4)
                            } else {
                                true
                            };

                            if should_ping {
                                peers_to_ping.push(cached_peer.peer_id);
                            }
                        }
                    }

                    let message = LeanRequestMessage::Status(self.our_status());
                    for peer_id in peers_to_ping {
                        self.send_request(peer_id, message.clone());
                    }
                }
                Some(item) = self.outbound_p2p_request.recv() => {
                    match item {
                        LeanP2PRequest::GossipBlock(block) => {
                            self.publish_gossip(
                                |topic| matches!(topic, LeanGossipTopicKind::Block),
                                block.as_ssz_bytes(),
                                block.message.block.slot,
                                "block"
                            );
                        }
                        #[cfg(feature = "devnet2")]
                        LeanP2PRequest::GossipAttestation(attestation) => {
                            let slot = attestation.message.slot;
                            self.publish_gossip(
                                |topic| matches!(topic, LeanGossipTopicKind::Attestation),
                                attestation.as_ssz_bytes(),
                                slot,
                                "attestation"
                            );
                        }
                        #[cfg(feature = "devnet3")]
                        LeanP2PRequest::GossipAttestation { subnet_id, attestation } => {
                            let slot = attestation.message.slot;
                            self.publish_gossip_to_subnet(
                                subnet_id,
                                attestation.as_ssz_bytes(),
                                slot,
                                "attestation"
                            );
                        }
                        #[cfg(feature = "devnet3")]
                        LeanP2PRequest::GossipAggregatedAttestation(aggregated) => {
                            let slot = aggregated.data.slot;
                            self.publish_gossip(
                                |topic| matches!(topic, LeanGossipTopicKind::AggregatedAttestation),
                                aggregated.as_ssz_bytes(),
                                slot,
                                "aggregated_attestation"
                            );
                        }
                        LeanP2PRequest::Request { peer_id, callback, message } => {
                            let message = match message {
                                P2PCallbackRequest::BlocksByRoot { roots } => {
                                    LeanRequestMessage::BlocksByRoot(BlocksByRootV1Request::new(roots))
                                }
                                P2PCallbackRequest::Status => {
                                    LeanRequestMessage::Status(self.our_status())
                                }
                            };
                            match  self.send_request(peer_id, message) {
                                RequestResult::Success(request_id) => {
                                    self.chain_callback_requests.insert(request_id, (peer_id, callback));
                                },
                                RequestResult::NotConnected => {
                                   if let Err(err) =  callback.send(ResponseCallback::NotConnected { peer_id }).await {
                                        warn!("Failed to send not connected error to callback: {err:?}");
                                   }
                                },
                            }
                        }
                        LeanP2PRequest::Response { peer_id, stream_id, connection_id, message } => {
                            self.send_response(peer_id, connection_id, stream_id, message);
                        }
                        LeanP2PRequest::EndOfStream { peer_id, stream_id, connection_id } => {
                            self.send_end_of_stream(peer_id, connection_id, stream_id);
                        }
                    }
                }
                Some(event) = self.swarm.next() => {
                    if let Some(event) = self.parse_swarm_event(event).await {
                        trace!("Swarm event: {event:?}");
                        match event {
                            ReamNetworkEvent::Event(network_event) => {
                                if let Err(err) = self.chain_message_sender.send(LeanChainServiceMessage::NetworkEvent(network_event)) {
                                    warn!("failed to send network event to chain: {err:?}");
                                }
                            },
                            ReamNetworkEvent::ResponseCallback(response_callback) => {
                                let request_id = match &response_callback {
                                    ResponseCallback::ResponseMessage { request_id, .. } => *request_id,
                                    ResponseCallback::EndOfStream { request_id, .. } => *request_id,
                                    ResponseCallback::NotConnected { .. } => {
                                        warn!("Received NotConnected response callback, which should not happen here.");
                                        continue;
                                    }
                                };

                                let callback_sender = match &response_callback {
                                    ResponseCallback::EndOfStream { .. } => {
                                        self.chain_callback_requests.remove(&request_id).map(|(_, sender)| sender)
                                    },
                                    _ => {
                                        self.chain_callback_requests.get(&request_id).map(|(_, sender)| sender.clone())
                                    }
                                };

                                if let Some(callback) = callback_sender {
                                    match response_callback {
                                        ResponseCallback::ResponseMessage { peer_id, request_id, message } => {
                                            if let Err(err) = callback.send(ResponseCallback::ResponseMessage { peer_id, message, request_id }).await {
                                                warn!("Failed to send response message to callback: {err:?}");
                                            }
                                        },
                                        ResponseCallback::EndOfStream { peer_id, request_id } => {
                                            if let Err(err) = callback.send(ResponseCallback::EndOfStream { peer_id, request_id }).await {
                                                warn!("Failed to send end of stream to callback: {err:?}");
                                            }
                                        },
                                        ResponseCallback::NotConnected { peer_id } => {
                                            warn!("Received NotConnected response callback for peer {peer_id:?}, which should not happen here.");
                                        }
                                    }
                                } else {
                                    match response_callback {
                                        ResponseCallback::EndOfStream { .. } => {
                                        }
                                        _ => {
                                            error!("No callback found for request_id: {request_id}");
                                        }
                                    }
                                }
                            },
                        }
                    }
                }
                Some(result) = self.check_canonical_futures.next() => {
                    match result {
                        Ok((peer_id, is_canonical)) => {
                            if is_canonical {
                                info!(
                                    ?peer_id,
                                    "Peer has canonical checkpoint"
                                );
                            } else {
                                warn!(
                                    ?peer_id,
                                    "Peer does not have canonical checkpoint, disconnecting"
                                );
                                if let Err(err) = self.swarm.disconnect_peer_id(peer_id) {
                                    warn!("Failed to disconnect peer: {err:?}");
                                }
                            }
                        }
                        Err(err) => {
                            warn!("Failed to receive CheckIfCanonicalCheckpoint result: {err:?}");
                        }
                    }
                }
            }
        }
    }

    fn publish_gossip<F>(&mut self, topic_filter: F, data: Vec<u8>, slot: u64, name: &str)
    where
        F: Fn(&LeanGossipTopicKind) -> bool,
    {
        let topic = self
            .network_config
            .gossipsub_config
            .topics
            .iter()
            .find(|topic| topic_filter(&topic.kind))
            .map(|topic| IdentTopic::from(topic.clone()))
            .unwrap_or_else(|| panic!("Lean{name} topic configured"));

        match self.swarm.behaviour_mut().gossipsub.publish(topic, data) {
            Ok(_) => info!(slot, "Broadcasted {name}"),
            Err(PublishError::Duplicate) => {
                trace!(slot, "{name} already published (duplicate)");
            }
            Err(err) => warn!(slot, ?err, "Publish {name} failed"),
        }
    }

    #[cfg(feature = "devnet3")]
    fn publish_gossip_to_subnet(&mut self, subnet_id: u64, data: Vec<u8>, slot: u64, name: &str) {
        let topic = self
            .network_config
            .gossipsub_config
            .topics
            .iter()
            .find(|topic| matches!(&topic.kind, LeanGossipTopicKind::AttestationSubnet(id) if *id == subnet_id))
            .map(|topic| IdentTopic::from(topic.clone()))
            .unwrap_or_else(|| panic!("Lean attestation subnet {subnet_id} topic not configured"));

        match self.swarm.behaviour_mut().gossipsub.publish(topic, data) {
            Ok(_) => info!(slot, subnet_id, "Broadcasted {name}"),
            Err(PublishError::Duplicate) => {
                trace!(slot, subnet_id, "{name} already published (duplicate)");
            }
            Err(err) => warn!(slot, subnet_id, ?err, "Publish {name} failed"),
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
                self.handle_request_response_event(req_resp_event).await
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                let (address, direction) = match endpoint {
                    ConnectedPoint::Dialer { address, .. } => {
                        self.bootnode_retry_state.remove(&peer_id);

                        // send status request to the peer
                        let status_message = LeanRequestMessage::Status(self.our_status());
                        self.send_request(peer_id, status_message);

                        (address, Direction::Outbound)
                    }
                    ConnectedPoint::Listener { send_back_addr, .. } => {
                        (send_back_addr, Direction::Inbound)
                    }
                };
                self.network_state.upsert_peer(
                    peer_id,
                    Some(address),
                    ConnectionState::Connected,
                    direction,
                );
                set_int_gauge_vec(
                    &LEAN_PEER_COUNT,
                    self.network_state.connected_peer_count() as i64,
                    &[],
                );
                inc_int_counter_vec(&LEAN_CONNECTION_EVENT_TOTAL, &[]);

                info!(
                    "Connected to peer: {peer_id:?} {:?}",
                    self.network_state.peer_table
                );
                None
            }
            SwarmEvent::ConnectionClosed {
                peer_id, endpoint, ..
            } => {
                let direction = match endpoint {
                    ConnectedPoint::Dialer { .. } => Direction::Outbound,
                    ConnectedPoint::Listener { .. } => Direction::Inbound,
                };
                self.network_state.upsert_peer(
                    peer_id,
                    None,
                    ConnectionState::Disconnected,
                    direction,
                );
                set_int_gauge_vec(
                    &LEAN_PEER_COUNT,
                    self.network_state.connected_peer_count() as i64,
                    &[],
                );
                inc_int_counter_vec(&LEAN_DISCONNECTION_EVENT_TOTAL, &[]);

                info!("Disconnected from peer: {peer_id:?}");
                None
            }
            SwarmEvent::IncomingConnection { local_addr, .. } => {
                info!("Incoming connection from {local_addr:?}");
                None
            }
            SwarmEvent::Dialing { peer_id, .. } => {
                info!("Dialing {peer_id:?}");
                None
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
                Ok(LeanGossipsubMessage::Block(signed_block_with_attestation)) => {
                    let slot = signed_block_with_attestation.message.block.slot;

                    if let Err(err) =
                        self.chain_message_sender
                            .send(LeanChainServiceMessage::ProcessBlock {
                                signed_block_with_attestation,
                                need_gossip: true,
                            })
                    {
                        warn!("failed to send block for slot {slot} item to chain: {err:?}");
                    }
                }
                #[cfg(feature = "devnet2")]
                Ok(LeanGossipsubMessage::Attestation(signed_attestation)) => {
                    let slot = signed_attestation.message.slot;

                    if let Err(err) = self.chain_message_sender.send(
                        LeanChainServiceMessage::ProcessAttestation {
                            signed_attestation,
                            need_gossip: true,
                        },
                    ) {
                        warn!("failed to send attestation for slot {slot} to chain: {err:?}");
                    }
                }
                #[cfg(feature = "devnet3")]
                Ok(LeanGossipsubMessage::Attestation {
                    subnet_id,
                    attestation: signed_attestation,
                }) => {
                    let slot = signed_attestation.message.slot;

                    if let Err(err) = self.chain_message_sender.send(
                        LeanChainServiceMessage::ProcessAttestation {
                            signed_attestation,
                            subnet_id,
                            need_gossip: true,
                        },
                    ) {
                        warn!(
                            "failed to send attestation for slot {slot} subnet {subnet_id} to chain: {err:?}"
                        );
                    }
                }
                #[cfg(feature = "devnet3")]
                Ok(LeanGossipsubMessage::AggregatedAttestation(aggregated_attestation)) => {
                    let slot = aggregated_attestation.data.slot;

                    if let Err(err) = self.chain_message_sender.send(
                        LeanChainServiceMessage::ProcessAggregatedAttestation {
                            aggregated_attestation,
                            need_gossip: true,
                        },
                    ) {
                        warn!(
                            "failed to send aggregated attestation for slot {slot} to chain: {err:?}"
                        );
                    }
                }
                Err(err) => warn!("Failed to decode {:?} gossip topic: {err:?}", message.topic),
            }
        }
        None
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
            .lock()
            .entry(peer_id)
            .and_modify(|cached_peer| {
                cached_peer.update_last_seen();
            });

        let message = match message {
            Ok(message) => message,
            Err(err) => {
                warn!(
                    ?peer_id,
                    ?connection_id,
                    "Failed to parse req/resp message from peer: {err:?}"
                );
                return Some(ReamNetworkEvent::Event(NetworkEvent::NetworkError {
                    peer_id,
                    error: err,
                }));
            }
        };

        match message {
            ReqRespMessageReceived::Request { stream_id, message } => {
                if let RequestMessage::Lean(message) = *message {
                    match message {
                        LeanRequestMessage::Status(status) => {
                            trace!(
                                ?peer_id,
                                ?stream_id,
                                ?connection_id,
                                ?status,
                                "Received Status request"
                            );

                            self.handle_status_response(peer_id, &status);

                            let our_status = self.our_status();
                            self.send_response(
                                peer_id,
                                connection_id,
                                stream_id,
                                LeanResponseMessage::Status(our_status),
                            );
                            self.send_end_of_stream(peer_id, connection_id, stream_id);

                            // We handle this internally, so no need to forward to chain
                            None
                        }
                        _ => Some(ReamNetworkEvent::Event(NetworkEvent::RequestMessage {
                            peer_id,
                            stream_id,
                            connection_id,
                            message,
                        })),
                    }
                } else {
                    warn!(
                        "Received unexpected Beacon request message: {:?} from peer: {:?}",
                        message, peer_id
                    );
                    None
                }
            }
            ReqRespMessageReceived::Response {
                request_id,
                message,
            } => {
                if let ResponseMessage::Lean(response_message) = *message {
                    if let LeanResponseMessage::Status(status) = &*response_message {
                        trace!(
                            ?peer_id,
                            ?request_id,
                            "Received Status response: head_hash: {}, head_slot: {}",
                            status.head.root,
                            status.head.slot
                        );

                        self.handle_status_response(peer_id, status);
                        // We handle this internally, so no need to forward to chain
                        return None;
                    }
                    return Some(ReamNetworkEvent::ResponseCallback(
                        ResponseCallback::ResponseMessage {
                            peer_id,
                            request_id,
                            message: response_message,
                        },
                    ));
                } else {
                    warn!(
                        "Received unexpected Beacon response message: {message:?} from peer: {peer_id:?}"
                    );
                }

                None
            }
            ReqRespMessageReceived::EndOfStream { request_id } => Some(
                ReamNetworkEvent::ResponseCallback(ResponseCallback::EndOfStream {
                    peer_id,
                    request_id,
                }),
            ),
        }
    }

    async fn connect_to_multinodes(&mut self, peers: Vec<Multiaddr>) {
        trace!("Discovered peers: {peers:?}");
        for peer in peers {
            if let Some(Protocol::P2p(peer_id)) = peer
                .iter()
                .find(|protocol| matches!(protocol, Protocol::P2p(_)))
                && peer_id != self.local_peer_id()
            {
                if let Some(cached_peer) = self.network_state.cached_peer(&peer_id)
                    && matches!(cached_peer.state, ConnectionState::Connected)
                {
                    trace!("Already connected to peer {peer_id}, skipping dial.");
                    continue;
                }

                match self.bootnode_retry_state.remove(&peer_id) {
                    Some((attempts, mut addresses)) => {
                        addresses.push(peer.clone());
                        self.bootnode_retry_state
                            .insert(peer_id, (attempts, addresses));
                    }
                    None => self
                        .bootnode_retry_state
                        .insert(peer_id, (0, vec![peer.clone()])),
                };

                if let Err(err) = self.dial_peer(peer.clone()) {
                    warn!("Failed to dial peer: {err:?}");
                    continue;
                }

                info!("Dialing peer: {peer_id:?}");
                self.network_state.upsert_peer(
                    peer_id,
                    Some(peer),
                    ConnectionState::Connecting,
                    Direction::Outbound,
                );
            }
        }
    }

    pub fn handle_status_response(&mut self, peer_id: PeerId, status: &Status) {
        info!(
            ?peer_id,
            head_slot = status.head.slot,
            finalized_slot = status.finalized.slot,
            "Received status response from peer"
        );

        self.network_state
            .update_peer_checkpoints(peer_id, status.head, status.finalized);

        let (sender, receiver) = oneshot::channel();
        match self
            .chain_message_sender
            .send(LeanChainServiceMessage::CheckIfCanonicalCheckpoint {
                peer_id,
                checkpoint: status.finalized,
                sender,
            }) {
            Ok(_) => self.check_canonical_futures.push(receiver),
            Err(err) => warn!(
                ?peer_id,
                finalized_slot = status.finalized.slot,
                "Failed to send CheckIfCanonicalCheckpoint request: {err:?}"
            ),
        }
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    fn dial_peer(&mut self, peer_addr: Multiaddr) -> anyhow::Result<()> {
        self.swarm
            .dial(peer_addr.clone())
            .map_err(|err| anyhow!("Failed to dial peer at address {peer_addr:?}, error: {err:?}"))
    }

    fn send_request(&mut self, peer_id: PeerId, message: LeanRequestMessage) -> RequestResult<u64> {
        if !self.swarm.is_connected(&peer_id) {
            return RequestResult::NotConnected;
        }

        let request_id = self.request_id();
        self.swarm.behaviour_mut().req_resp.send_request(
            peer_id,
            request_id,
            RequestMessage::Lean(message),
        );

        RequestResult::Success(request_id)
    }

    fn request_id(&mut self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    fn send_response(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
        message: LeanResponseMessage,
    ) {
        self.swarm.behaviour_mut().req_resp.send_response(
            peer_id,
            connection_id,
            stream_id,
            RespMessage::Response(Box::new(ResponseMessage::Lean(message.into()))),
        );
    }

    fn send_end_of_stream(&mut self, peer_id: PeerId, connection_id: ConnectionId, stream_id: u64) {
        self.swarm.behaviour_mut().req_resp.send_response(
            peer_id,
            connection_id,
            stream_id,
            RespMessage::EndOfStream,
        );
    }

    fn our_status(&self) -> Status {
        Status {
            finalized: *self.network_state.finalized_checkpoint.read(),
            head: *self.network_state.head_checkpoint.read(),
        }
    }

    /// Returns the cached peer from the peer table.
    pub fn cached_peer(&self, id: &PeerId) -> Option<CachedPeer> {
        self.network_state.peer_table.lock().get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, time::Duration};

    use alloy_primitives::B256;
    use ream_consensus_lean::checkpoint::Checkpoint;
    use ream_network_spec::networks::initialize_lean_test_network_spec;
    use tokio::sync::mpsc;
    use tracing_test::traced_test;

    use super::*;
    use crate::bootnodes::Bootnodes;

    pub async fn setup_lean_node(
        socket_port: u16,
    ) -> anyhow::Result<(
        LeanNetworkService,
        UnboundedSender<LeanP2PRequest>,
        UnboundedReceiver<LeanChainServiceMessage>,
    )> {
        initialize_lean_test_network_spec();

        let executor = ReamExecutor::new().expect("Failed to create executor");
        let config = Arc::new(LeanNetworkConfig {
            gossipsub_config: LeanGossipsubConfig::default(),
            socket_address: Ipv4Addr::new(127, 0, 0, 1).into(),
            socket_port,
            private_key_path: None,
        });

        let (chain_sender, chain_receiver) = mpsc::unbounded_channel::<LeanChainServiceMessage>();
        let (outbound_request_sender, outbound_request_receiver) =
            mpsc::unbounded_channel::<LeanP2PRequest>();

        let node = LeanNetworkService::new(
            config.clone(),
            executor,
            chain_sender,
            outbound_request_receiver,
            Arc::new(NetworkState::new(Default::default(), Default::default())),
        )
        .await?;

        Ok((node, outbound_request_sender, chain_receiver))
    }

    #[tokio::test]
    #[traced_test]
    async fn test_two_quic_lean_nodes_connection() -> anyhow::Result<()> {
        let socket_port1 = 9000;
        let socket_port2 = 9001;

        let (mut node_1, _, _) = setup_lean_node(socket_port1).await?;
        let (mut node_2, _, _) = setup_lean_node(socket_port2).await?;

        let peer_id_network_1 = node_1.local_peer_id();
        let peer_id_network_2 = node_2.local_peer_id();

        let network_state_1 = node_1.network_state.clone();
        let network_state_2 = node_2.network_state.clone();

        let node_1_addr = node_1.multi_addr.clone();

        let node_1_handle = tokio::spawn(async move {
            let bootnodes = Bootnodes::Default;
            node_1.start(bootnodes).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let node_2_handle = tokio::spawn(async move {
            let bootnodes = Bootnodes::Multiaddr(vec![node_1_addr]);
            node_2.start(bootnodes).await.unwrap();
        });

        tokio::time::sleep(Duration::from_secs(2)).await;

        node_1_handle.abort();
        node_2_handle.abort();

        let peer_from_network_1 = network_state_1
            .cached_peer(&peer_id_network_2)
            .expect("network_1 peer exists");
        let peer_from_network_2 = network_state_2
            .cached_peer(&peer_id_network_1)
            .expect("network_2 peer exists");

        assert_eq!(peer_from_network_1.state, ConnectionState::Connected);
        assert_eq!(peer_from_network_1.direction, Direction::Inbound);

        assert_eq!(peer_from_network_2.state, ConnectionState::Connected);
        assert_eq!(peer_from_network_2.direction, Direction::Outbound);

        Ok(())
    }

    #[tokio::test]
    #[traced_test]
    async fn test_request_status_and_response() -> anyhow::Result<()> {
        let (mut node_1, p2p_sender_1, mut chain_receiver_1) = setup_lean_node(9002).await?;
        let (mut node_2, _p2p_sender_2, _chain_receiver_2) = setup_lean_node(9003).await?;

        let peer_id_2 = node_2.local_peer_id();
        let node_1_addr = node_1.multi_addr.clone();

        let expected_head = Checkpoint {
            root: B256::repeat_byte(0xaa),
            slot: 100,
        };
        let expected_finalized = Checkpoint {
            root: B256::repeat_byte(0xbb),
            slot: 50,
        };

        *node_2.network_state.head_checkpoint.write() = expected_head;
        *node_2.network_state.finalized_checkpoint.write() = expected_finalized;

        let network_state_1 = node_1.network_state.clone();

        let node_1_handle = tokio::spawn(async move {
            let bootnodes = Bootnodes::Default;
            node_1.start(bootnodes).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let node_2_handle = tokio::spawn(async move {
            let bootnodes = Bootnodes::Multiaddr(vec![node_1_addr]);
            node_2.start(bootnodes).await.unwrap();
        });

        tokio::time::sleep(Duration::from_secs(2)).await;

        let (callback, _) = mpsc::channel(5);
        p2p_sender_1.send(LeanP2PRequest::Request {
            peer_id: peer_id_2,
            callback,
            message: P2PCallbackRequest::Status,
        })?;

        tokio::time::sleep(Duration::from_secs(1)).await;

        let peer_2_state = network_state_1
            .cached_peer(&peer_id_2)
            .expect("Peer 2 should be in Node 1's peer table");

        assert_eq!(
            peer_2_state.head_checkpoint,
            Some(expected_head),
            "Head checkpoint should match"
        );
        assert_eq!(
            peer_2_state.finalized_checkpoint,
            Some(expected_finalized),
            "Finalized checkpoint should match"
        );

        let message = tokio::time::timeout(Duration::from_millis(100), chain_receiver_1.recv())
            .await
            .map_err(|err| anyhow!("Timeout waiting for chain message: {err:?}"))?
            .ok_or(anyhow!("Channel closed"))?;

        if let LeanChainServiceMessage::CheckIfCanonicalCheckpoint {
            peer_id,
            checkpoint,
            ..
        } = message
        {
            assert_eq!(peer_id, peer_id_2);
            assert_eq!(checkpoint, expected_finalized);
        } else {
            panic!("Unexpected message: {message:?}");
        }

        node_1_handle.abort();
        node_2_handle.abort();

        Ok(())
    }
}
