use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use anyhow::anyhow;
use discv5::{
    Discv5, Enr,
    enr::{CombinedKey, NodeId, k256::ecdsa::SigningKey},
};
use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    identity::Keypair,
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm, dummy::ConnectionHandler,
    },
};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::{
    config::NetworkConfig,
    subnet::{ATTESTATION_BITFIELD_ENR_KEY, Subnet, Subnets, subnet_predicate},
};

#[derive(Debug)]
pub struct DiscoveredPeers {
    pub peers: HashMap<Enr, Option<Instant>>,
}

enum EventStream {
    Inactive,
    Awaiting(
        Pin<Box<dyn Future<Output = Result<mpsc::Receiver<discv5::Event>, discv5::Error>> + Send>>,
    ),
    Present(mpsc::Receiver<discv5::Event>),
}

#[derive(Debug, Clone, PartialEq)]
enum QueryType {
    FindPeers,
    FindSubnetPeers(Vec<Subnet>),
}

struct QueryResult {
    query_type: QueryType,
    result: Result<Vec<Enr>, discv5::QueryError>,
}

pub struct Discovery {
    discv5: Discv5,
    event_stream: EventStream,
    discovery_queries: FuturesUnordered<Pin<Box<dyn Future<Output = QueryResult> + Send>>>,
    find_peer_active: bool,
    pub started: bool,
    subnets: Subnets,
}

impl Discovery {
    pub async fn new(
        local_key: libp2p::identity::Keypair,
        config: &NetworkConfig,
    ) -> anyhow::Result<Self> {
        let enr_key = convert_to_enr(local_key)
            .map_err(|e| anyhow::anyhow!("Failed to convert key: {:?}", e))?;
        let mut enr_builder = Enr::builder();
        enr_builder.ip(config.socket_address);
        enr_builder.udp4(config.socket_port);
        if let Some(attestation_bytes) = config.subnets.attestation_bytes() {
            enr_builder.add_value(ATTESTATION_BITFIELD_ENR_KEY, &attestation_bytes);
        }
        let enr = enr_builder
            .build(&enr_key)
            .map_err(|e| anyhow::anyhow!("Failed to build ENR: {:?}", e))?;
        let node_local_id = enr.node_id();

        let mut discv5 = Discv5::new(enr, enr_key, config.discv5_config.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create discv5: {:?}", e))?;

        for bootnode_enr in config.bootnodes.clone() {
            if bootnode_enr.node_id() == node_local_id {
                continue;
            }
            discv5
                .add_enr(bootnode_enr)
                .map_err(|e| anyhow::anyhow!("Failed to add bootnode: {:?}", e))?;
        }

        let event_stream = if !config.disable_discovery {
            discv5
                .start()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to start discv5: {:?}", e))?;
            info!("Started discovery with ENR: {:?}", discv5.local_enr());
            EventStream::Awaiting(Box::pin(discv5.event_stream()))
        } else {
            EventStream::Inactive
        };

        Ok(Self {
            discv5,
            event_stream,
            discovery_queries: FuturesUnordered::new(),
            find_peer_active: false,
            started: !config.disable_discovery,
            subnets: config.subnets.clone(),
        })
    }

    pub fn discover_peers(&mut self, target_peers: usize) {
        // If the discv5 service isn't running or we are in the process of a query, don't bother
        // queuing a new one.
        if !self.started || self.find_peer_active {
            return;
        }
        self.find_peer_active = true;
        self.start_query(QueryType::FindPeers, target_peers);
    }

    pub fn discover_subnet_peers(&mut self, subnet_id: u8, target_peers: usize) {
        if !self.started || self.find_peer_active {
            return;
        }
        self.find_peer_active = true;
        self.start_query(
            QueryType::FindSubnetPeers(vec![Subnet::Attestation(subnet_id)]),
            target_peers,
        );
    }

    pub fn update_attestation_subnet(&mut self, subnet_id: u8, value: bool) -> Result<(), String> {
        let mut current_subnets = self.subnets.clone();
        match Subnet::Attestation(subnet_id) {
            // Use Subnet enum
            Subnet::Attestation(id) if id < 64 => {
                if current_subnets.is_active(Subnet::Attestation(id)) == value {
                    return Ok(()); // No change needed
                }
                if value {
                    current_subnets.enable_subnet(Subnet::Attestation(id));
                } else {
                    current_subnets.disable_subnet(Subnet::Attestation(id));
                }
                if let Some(bytes) = current_subnets.attestation_bytes() {
                    self.discv5
                        .enr_insert(ATTESTATION_BITFIELD_ENR_KEY, &bytes)
                        .map_err(|e| format!("Failed to update ENR attnets: {:?}", e))?;
                }
            }
            Subnet::Attestation(_) => {
                return Err(format!(
                    "Subnet ID {} exceeds bitfield length 64",
                    subnet_id
                ));
            }
            Subnet::SyncCommittee(_) => unimplemented!("SyncCommittee support not yet implemented"),
        }
        self.subnets = current_subnets;
        info!(
            "Updated ENR attnets: {:?}",
            self.subnets.attestation_bytes()
        );
        Ok(())
    }

    fn start_query(&mut self, query: QueryType, target_peers: usize) {
        let query_future: Pin<Box<dyn Future<Output = QueryResult> + Send>> = match query {
            QueryType::FindPeers => Box::pin({
                let query_clone = query.clone();
                self.discv5
                    .find_node(NodeId::random())
                    .map(move |result| QueryResult {
                        query_type: query_clone,
                        result,
                    })
            }),
            QueryType::FindSubnetPeers(ref subnets) => {
                let predicate = subnet_predicate(subnets.clone());
                Box::pin(
                    self.discv5
                        .find_node_predicate(NodeId::random(), Box::new(predicate), target_peers)
                        .map(|result| QueryResult {
                            query_type: query,
                            result,
                        }),
                )
            }
        };
        self.discovery_queries.push(query_future);
    }

    fn process_queries(&mut self, cx: &mut Context) -> Option<HashMap<Enr, Option<Instant>>> {
        while let Poll::Ready(Some(query)) = self.discovery_queries.poll_next_unpin(cx) {
            let result = match query.query_type {
                QueryType::FindPeers => {
                    self.find_peer_active = false;
                    match query.result {
                        Ok(peers) => {
                            info!("Found {} peers", peers.len());
                            let mut peer_map = HashMap::new();
                            for peer in peers {
                                peer_map.insert(peer, None);
                            }
                            Some(peer_map)
                        }
                        Err(e) => {
                            warn!("Failed to find peers: {:?}", e);
                            None
                        }
                    }
                }
                QueryType::FindSubnetPeers(subnets) => {
                    self.find_peer_active = false;
                    match query.result {
                        Ok(peers) => {
                            let predicate = subnet_predicate(subnets.clone());
                            let filtered_peers = peers
                                .into_iter()
                                .filter(|enr| predicate(enr))
                                .collect::<Vec<_>>();
                            info!(
                                "Found {} peers for subnets {:?}",
                                filtered_peers.len(),
                                subnets
                            );
                            let mut peer_map = HashMap::new();
                            for peer in filtered_peers {
                                peer_map.insert(peer, None);
                            }
                            Some(peer_map)
                        }
                        Err(e) => {
                            warn!("Failed to find subnet peers: {:?}", e);
                            None
                        }
                    }
                }
            };
            if result.is_some() {
                return result;
            }
        }
        None
    }
}

impl NetworkBehaviour for Discovery {
    type ConnectionHandler = ConnectionHandler;
    type ToSwarm = DiscoveredPeers;

    fn handle_pending_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        Ok(())
    }

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        info!("Discv5 on swarm event gotten: {:?}", event);
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if !self.started {
            return Poll::Pending;
        }

        if let Some(peers) = self.process_queries(cx) {
            return Poll::Ready(ToSwarm::GenerateEvent(DiscoveredPeers { peers }));
        }

        match &mut self.event_stream {
            EventStream::Inactive => {}
            EventStream::Awaiting(fut) => {
                if let Poll::Ready(event_stream) = fut.poll_unpin(cx) {
                    match event_stream {
                        Ok(stream) => {
                            self.event_stream = EventStream::Present(stream);
                        }
                        Err(e) => {
                            error!("Failed to start discovery event stream: {:?}", e);
                            self.event_stream = EventStream::Inactive;
                        }
                    }
                }
            }
            EventStream::Present(_receiver) => {}
        };

        Poll::Pending
    }
}

fn convert_to_enr(key: Keypair) -> anyhow::Result<CombinedKey> {
    let key = key
        .try_into_secp256k1()
        .map_err(|err| anyhow!("Failed to get secp256k1 keypair: {err:?}"))?;
    let secret = SigningKey::from_slice(&key.secret().to_bytes())
        .map_err(|err| anyhow!("Failed to convert keypair to SigningKey: {err:?}"))?;
    Ok(CombinedKey::Secp256k1(secret))
}
