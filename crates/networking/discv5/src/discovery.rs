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
use ream_consensus::constants::genesis_validators_root;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::{
    config::DiscoveryConfig,
    eth2::{ENR_ETH2_KEY, EnrForkId},
    subnet::{
        ATTESTATION_BITFIELD_ENR_KEY, SYNC_COMMITTEE_BITFIELD_ENR_KEY,
        attestation_subnet_predicate, sync_committee_subnet_predicate,
    },
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
pub enum QueryType {
    Peers,
    AttestationSubnetPeers(Vec<u8>),
    SyncCommitteeSubnetPeers(Vec<u8>),
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
}

impl Discovery {
    pub async fn new(local_key: Keypair, config: &DiscoveryConfig) -> anyhow::Result<Self> {
        let enr_local =
            convert_to_enr(local_key).map_err(|err| anyhow!("Failed to convert key: {err:?}"))?;

        let mut enr_builder = Enr::builder();
        enr_builder.ip(config.socket_address);
        enr_builder.tcp4(config.socket_port);
        enr_builder.udp4(config.discovery_port);

        let enr = enr_builder
            .add_value(ENR_ETH2_KEY, &EnrForkId::electra(genesis_validators_root()))
            .add_value(ATTESTATION_BITFIELD_ENR_KEY, &config.attestation_subnets)
            .add_value(
                SYNC_COMMITTEE_BITFIELD_ENR_KEY,
                &config.sync_committee_subnets,
            )
            .build(&enr_local)
            .map_err(|err| anyhow!("Failed to build ENR: {err}"))?;

        let node_local_id = enr.node_id();

        let mut discv5 = Discv5::new(enr.clone(), enr_local, config.discv5_config.clone())
            .map_err(|err| anyhow!("Failed to create discv5: {err:?}"))?;

        // adding bootnodes to discv5
        for enr in config.bootnodes.clone() {
            // Skip adding ourselves to the routing table if we are a bootnode
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
                .map_err(|err| anyhow!("Failed to start discv5: {err:?}"))?;
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
        })
    }

    pub fn local_enr(&self) -> &Enr {
        &self.local_enr
    }

    pub fn discover_peers(&mut self, query: QueryType, target_peers: usize) {
        // If the discv5 service isn't running or we are in the process of a query, don't bother
        // queuing a new one.
        if !self.started || self.find_peer_active {
            return;
        }
        self.find_peer_active = true;

        self.start_query(query, target_peers);
    }

    fn start_query(&mut self, query: QueryType, target_peers: usize) {
        let query_future = self
            .discv5
            .find_node_predicate(
                NodeId::random(),
                match query.clone() {
                    QueryType::Peers => Box::new(empty_predicate()),
                    QueryType::AttestationSubnetPeers(subnet_ids) => {
                        Box::new(attestation_subnet_predicate(subnet_ids))
                    }
                    QueryType::SyncCommitteeSubnetPeers(subnet_ids) => {
                        Box::new(sync_committee_subnet_predicate(subnet_ids))
                    }
                },
                target_peers,
            )
            .map(move |result| QueryResult {
                query_type: query,
                result,
            });

        self.discovery_queries.push(Box::pin(query_future));
    }

    fn process_queries(&mut self, cx: &mut Context) -> Option<HashMap<Enr, Option<Instant>>> {
        while let Poll::Ready(Some(query)) = self.discovery_queries.poll_next_unpin(cx) {
            let result = match query.query_type {
                QueryType::Peers => {
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
                QueryType::AttestationSubnetPeers(subnet_ids) => {
                    self.find_peer_active = false;
                    match query.result {
                        Ok(peers) => {
                            let predicate = attestation_subnet_predicate(subnet_ids);
                            let filtered_peers = peers
                                .into_iter()
                                .filter(|enr| predicate(enr))
                                .collect::<Vec<_>>();
                            info!("Found {} peers for subnets", filtered_peers.len());
                            let mut peer_map = HashMap::new();
                            for peer in filtered_peers {
                                peer_map.insert(peer, None);
                            }
                            Some(peer_map)
                        }
                        Err(err) => {
                            warn!("Failed to find subnet peers: {err:?}");
                            None
                        }
                    }
                }
                QueryType::SyncCommitteeSubnetPeers(subnet_ids) => {
                    self.find_peer_active = false;
                    match query.result {
                        Ok(peers) => {
                            let predicate = sync_committee_subnet_predicate(subnet_ids);
                            let filtered_peers = peers
                                .into_iter()
                                .filter(|enr| predicate(enr))
                                .collect::<Vec<_>>();
                            info!(
                                "Found {} peers for sync committee subnets",
                                filtered_peers.len(),
                            );
                            let mut peer_map = HashMap::new();
                            for peer in filtered_peers {
                                peer_map.insert(peer, None);
                            }
                            Some(peer_map)
                        }
                        Err(err) => {
                            warn!("Failed to find sync committee subnet peers: {err:?}");
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

pub fn empty_predicate() -> impl Fn(&Enr) -> bool + Send + Sync {
    move |_enr: &Enr| true
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

    use alloy_primitives::B256;
    use libp2p::identity::Keypair;
    use ream_consensus::constants::GENESIS_VALIDATORS_ROOT;
    use ream_network_spec::networks::{DEV, set_network_spec};

    use super::*;
    use crate::{
        config::DiscoveryConfig,
        subnet::{AttestationSubnets, SyncCommitteeSubnets},
    };

    #[tokio::test]
    async fn test_initial_subnet_setup() -> anyhow::Result<()> {
        let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
        set_network_spec(DEV.clone());
        let key = Keypair::generate_secp256k1();
        let mut config = DiscoveryConfig::default();
        config.attestation_subnets.enable_attestation_subnet(0)?; // Set subnet 0
        config.attestation_subnets.disable_attestation_subnet(1)?; // Set subnet 1
        config.disable_discovery = true;

        let discovery = Discovery::new(key, &config).await.unwrap();
        let enr: &discv5::enr::Enr<CombinedKey> = discovery.local_enr();
        // Check ENR reflects config.subnets
        let enr_subnets = enr
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .ok_or("ATTESTATION_BITFIELD_ENR_KEY not found")
            .map_err(|err| anyhow!("ATTESTATION_BITFIELD_ENR_KEY decoding failed: {err:?}"))??;
        assert!(enr_subnets.is_attestation_subnet_enabled(0)?);
        assert!(!enr_subnets.is_attestation_subnet_enabled(1)?);
        Ok(())
    }

    #[tokio::test]
    async fn test_attestation_subnet_predicate() -> anyhow::Result<()> {
        let key = Keypair::generate_secp256k1();
        let mut config = DiscoveryConfig::default();
        config.attestation_subnets.enable_attestation_subnet(0)?; // Local node on subnet 0
        config.attestation_subnets.disable_attestation_subnet(1)?;
        config.disable_discovery = true;

        let discovery = Discovery::new(key, &config).await.unwrap();
        let local_enr = discovery.local_enr();

        // Predicate for subnet 0 should match
        let predicate = attestation_subnet_predicate(vec![0]);
        assert!(predicate(local_enr));

        // Predicate for subnet 1 should not match
        let predicate = attestation_subnet_predicate(vec![1]);
        assert!(!predicate(local_enr));
        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_with_subnets() -> anyhow::Result<()> {
        let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
        let key = Keypair::generate_secp256k1();
        let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::default())
            .table_filter(|_| true)
            .build();

        let mut config = DiscoveryConfig {
            disable_discovery: false,
            discv5_config: discv5_config.clone(),
            ..DiscoveryConfig::default()
        };

        config.attestation_subnets.enable_attestation_subnet(0)?; // Local node on subnet 0
        config.disable_discovery = false;
        let mut discovery = Discovery::new(key, &config).await.unwrap();

        // Simulate a peer with another Discovery instance
        let peer_key = Keypair::generate_secp256k1();
        let mut peer_config = DiscoveryConfig {
            attestation_subnets: AttestationSubnets::new(),
            sync_committee_subnets: SyncCommitteeSubnets::new(),
            disable_discovery: true,
            discv5_config,
            ..DiscoveryConfig::default()
        };

        peer_config
            .attestation_subnets
            .enable_attestation_subnet(0)?;
        peer_config.socket_address = Ipv4Addr::new(192, 168, 1, 100).into(); // Non-localhost IP
        peer_config.socket_port = 9001; // Different port
        peer_config.disable_discovery = true;

        let peer_discovery = Discovery::new(peer_key, &peer_config).await.unwrap();
        let peer_enr = peer_discovery.local_enr().clone();

        // Add peer to discv5
        discovery.discv5.add_enr(peer_enr.clone()).unwrap();

        // Mock the query result to bypass async polling
        discovery.discovery_queries.clear();
        let query_result = QueryResult {
            query_type: QueryType::AttestationSubnetPeers(vec![0]),
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
}
