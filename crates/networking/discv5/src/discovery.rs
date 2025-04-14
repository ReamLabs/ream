use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use alloy_rlp::Bytes;
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
    local_enr: Enr,
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
        // Need this test block to pass port and socket for test purpose
        #[cfg(test)]
        {
            enr_builder.ip(config.socket_address);
            enr_builder.udp4(config.socket_port);
        }
        if let Some(attestation_bytes) = config.subnets.attestation_bytes() {
            enr_builder.add_value::<Bytes>(ATTESTATION_BITFIELD_ENR_KEY, &attestation_bytes);
        }
        let enr = enr_builder
            .build(&enr_key)
            .map_err(|e| anyhow::anyhow!("Failed to build ENR: {:?}", e))?;
        let node_local_id = enr.node_id();

        let mut discv5 = Discv5::new(enr.clone(), enr_key, config.discv5_config.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create discv5: {:?}", e))?;

        for enr in config.bootnodes.clone() {
            if enr.node_id() == node_local_id {
                continue;
            }
            if let Err(err) = discv5.add_enr(enr) {
                error!("Failed to add bootnode to Discv5 {err:?}");
            };
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
            local_enr: enr,
            event_stream,
            discovery_queries: FuturesUnordered::new(),
            find_peer_active: false,
            started: !config.disable_discovery,
            subnets: config.subnets.clone(),
        })
    }

    pub fn local_enr(&self) -> &Enr {
        &self.local_enr
    }

    pub fn discover_peers(&mut self, target_peers: usize, subnet_id: Option<u8>) {
        // If the discv5 service isn't running or we are in the process of a query, don't bother
        // queuing a new one.
        if !self.started || self.find_peer_active {
            return;
        }
        self.find_peer_active = true;

        let query = match subnet_id {
            Some(id) => QueryType::FindSubnetPeers(vec![Subnet::Attestation(id)]),
            None => QueryType::FindPeers,
        };

        self.start_query(query, target_peers);
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
                    current_subnets.enable_subnet(Subnet::Attestation(id))?;
                } else {
                    current_subnets.disable_subnet(Subnet::Attestation(id))?;
                }
                if let Some(bytes) = current_subnets.attestation_bytes() {
                    self.discv5
                        .enr_insert::<Bytes>(ATTESTATION_BITFIELD_ENR_KEY, &bytes)
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
        let predicate = match query {
            QueryType::FindPeers => subnet_predicate(vec![]),
            QueryType::FindSubnetPeers(ref subnets) => subnet_predicate(subnets.clone()),
        };

        let query_future = self
            .discv5
            .find_node_predicate(NodeId::random(), Box::new(predicate), target_peers)
            .map(move |result| QueryResult {
                query_type: query,
                result,
            });
        self.discovery_queries.push(Box::pin(query_future));
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

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use libp2p::identity::Keypair;

    use super::*;
    use crate::config::NetworkConfig;

    #[tokio::test]
    async fn test_initial_subnet_setup() -> Result<(), String> {
        let key = Keypair::generate_secp256k1();
        let mut config = NetworkConfig::default();
        config.subnets.enable_subnet(Subnet::Attestation(0))?; // Set subnet 0
        config.subnets.disable_subnet(Subnet::Attestation(1))?; // Set subnet 0
        config.disable_discovery = true;

        let discovery = Discovery::new(key, &config).await.unwrap();
        let enr: &discv5::enr::Enr<CombinedKey> = discovery.local_enr();

        // Check ENR reflects config.subnets
        let enr_subnets = Subnets::from_enr(enr)?;
        assert!(enr_subnets.is_active(Subnet::Attestation(0)));
        assert!(!enr_subnets.is_active(Subnet::Attestation(1)));
        assert_eq!(
            discovery.subnets.attestation_bytes(),
            enr_subnets.attestation_bytes()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_subnet_update() -> Result<(), String> {
        let key = Keypair::generate_secp256k1();
        let mut config = NetworkConfig::default();
        config.subnets.enable_subnet(Subnet::Attestation(0))?; // Initial subnet 0
        config.disable_discovery = true;

        let mut discovery = Discovery::new(key, &config).await.unwrap();

        // Update to enable subnet 1, disable 0
        discovery.update_attestation_subnet(1, true).unwrap();
        discovery.update_attestation_subnet(0, false).unwrap();

        assert!(discovery.subnets.is_active(Subnet::Attestation(1)));
        assert!(!discovery.subnets.is_active(Subnet::Attestation(0)));

        let enr_subnets = Subnets::from_enr(discovery.local_enr())?;
        assert!(enr_subnets.is_active(Subnet::Attestation(1)));
        Ok(())
    }

    #[tokio::test]
    async fn test_subnet_predicate() -> Result<(), String> {
        let key = Keypair::generate_secp256k1();
        let mut config = NetworkConfig::default();
        config.subnets.enable_subnet(Subnet::Attestation(0))?; // Local node on subnet 0
        config.subnets.disable_subnet(Subnet::Attestation(1))?;
        config.disable_discovery = true;

        let discovery = Discovery::new(key, &config).await.unwrap();
        let local_enr = discovery.local_enr();

        // Predicate for subnet 0 should match
        let predicate = subnet_predicate(vec![Subnet::Attestation(0)]);
        assert!(predicate(local_enr));

        // Predicate for subnet 1 should not match
        let predicate = subnet_predicate(vec![Subnet::Attestation(1)]);
        assert!(!predicate(local_enr));
        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_with_subnets() -> Result<(), String> {
        let key = Keypair::generate_secp256k1();
        let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::default())
            .table_filter(|_| true)
            .build();

        let mut config = NetworkConfig {
            disable_discovery: false,
            discv5_config: discv5_config.clone(),
            ..NetworkConfig::default()
        };

        config.subnets.enable_subnet(Subnet::Attestation(0))?; // Local node on subnet 0
        config.disable_discovery = false;
        let mut discovery = Discovery::new(key, &config).await.unwrap();

        // Simulate a peer with another Discovery instance
        let peer_key = Keypair::generate_secp256k1();
        let mut peer_config = NetworkConfig {
            subnets: Subnets::new(),
            disable_discovery: true,
            discv5_config,
            ..NetworkConfig::default()
        };

        peer_config.subnets.enable_subnet(Subnet::Attestation(0))?;
        peer_config.socket_address = Ipv4Addr::new(192, 168, 1, 100).into(); // Non-localhost IP
        peer_config.socket_port = 9001; // Different port
        peer_config.disable_discovery = true;

        let peer_discovery = Discovery::new(peer_key, &peer_config).await.unwrap();
        let peer_enr = peer_discovery.local_enr().clone();

        // Add peer to discv5
        discovery.discv5.add_enr(peer_enr.clone()).unwrap();

        // Discover peers on subnet 0
        discovery.discover_peers(0, Some(1));

        // Mock the query result to bypass async polling
        discovery.discovery_queries.clear(); // Remove real query
        let query_result = QueryResult {
            query_type: QueryType::FindSubnetPeers(vec![Subnet::Attestation(0)]),
            result: Ok(vec![peer_enr.clone()]),
        };
        discovery
            .discovery_queries
            .push(Box::pin(async move { query_result }));

        // Poll the discovery to process the query
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        if let Poll::Ready(ToSwarm::GenerateEvent(DiscoveredPeers { peers })) =
            discovery.poll(&mut cx)
        {
            assert_eq!(peers.len(), 1);
            assert!(peers.contains_key(&peer_enr));
        } else {
            panic!("Expected peers to be discovered");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_subnet_update() {
        let key = Keypair::generate_secp256k1();
        let config = NetworkConfig {
            disable_discovery: true,
            ..NetworkConfig::default()
        };
        let mut discovery = Discovery::new(key, &config).await.unwrap();

        // Attempt to update with invalid subnet ID (64)
        let result = discovery.update_attestation_subnet(64, true);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Subnet ID 64 exceeds bitfield length 64"
        );
    }
}
