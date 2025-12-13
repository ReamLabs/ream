use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use alloy_rlp::Encodable;
use anyhow::{Result, anyhow};
use discv5::{
    Discv5, Enr, Event,
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
use ream_consensus_misc::{
    constants::beacon::genesis_validators_root, misc::compute_epoch_at_slot,
};
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

use crate::{
    config::DiscoveryConfig,
    eth2::{ENR_ETH2_KEY, EnrForkId},
    subnet::{
        ATTESTATION_BITFIELD_ENR_KEY, AttestationSubnets, CUSTODY_GROUP_COUNT_ENR_KEY,
        EPOCHS_PER_SUBNET_SUBSCRIPTION, NEXT_FORK_DIGEST_ENR_KEY, NextForkDigest,
        SYNC_COMMITTEE_BITFIELD_ENR_KEY, attestation_subnet_predicate, compute_subscribed_subnets,
        sync_committee_subnet_predicate,
    },
};

#[derive(Debug)]
pub enum DiscoveryOutEvent {
    DiscoveredPeers {
        peers: HashMap<Enr, Option<Instant>>,
    },
    UpdatedEnr {
        enr: Enr,
    },
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
    AttestationSubnetPeers(Vec<u64>),
    SyncCommitteeSubnetPeers(Vec<u64>),
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
    /// Current attestation subnets this node is subscribed to
    current_attestation_subnets: AttestationSubnets,
    /// The epoch at which the current subnet subscriptions were computed
    subscription_epoch: u64,
    /// Last slot we checked for subnet rotation
    last_checked_slot: u64,
}

impl Discovery {
    pub async fn new(
        local_key: Keypair,
        config: &DiscoveryConfig,
        current_slot: u64,
    ) -> anyhow::Result<Self> {
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
            .add_value(CUSTODY_GROUP_COUNT_ENR_KEY, &config.custody_group_count)
            .add_value(NEXT_FORK_DIGEST_ENR_KEY, &NextForkDigest::default())
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
            }
        }

        // Compute and set attestation subnets
        let subnets =
            compute_subscribed_subnets(enr.node_id(), compute_epoch_at_slot(current_slot))?;
        let mut config = config.clone();
        config.attestation_subnets = AttestationSubnets::new();
        for subnet_id in subnets {
            config
                .attestation_subnets
                .enable_attestation_subnet(subnet_id)?;
        }

        let subscription_epoch = compute_epoch_at_slot(current_slot);

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
            event_stream,
            discovery_queries: FuturesUnordered::new(),
            find_peer_active: false,
            started: !config.disable_discovery,
            current_attestation_subnets: config.attestation_subnets.clone(),
            subscription_epoch,
            last_checked_slot: current_slot,
        })
    }

    /// Update attestation subnet subscriptions based on the current slot
    /// This should be called periodically to rotate subnets according to the spec
    pub fn update_attestation_subnets(&mut self, current_slot: u64) -> Result<bool> {
        let current_epoch = compute_epoch_at_slot(current_slot);

        // Only update if we've moved to a new epoch since last check
        if current_slot <= self.last_checked_slot {
            return Ok(false);
        }

        self.last_checked_slot = current_slot;

        // Check if we need to rotate subscriptions
        // Subscriptions are valid for EPOCHS_PER_SUBNET_SUBSCRIPTION epochs
        let epochs_since_subscription = current_epoch.saturating_sub(self.subscription_epoch);

        if epochs_since_subscription < EPOCHS_PER_SUBNET_SUBSCRIPTION {
            return Ok(false);
        }

        debug!(
            "Rotating attestation subnet subscriptions at epoch {}, last subscription epoch: {}",
            current_epoch, self.subscription_epoch
        );

        // Compute new subnet subscriptions
        let node_id = self.discv5.local_enr().node_id();
        let new_subnets = compute_subscribed_subnets(node_id, current_epoch)?;

        // Build new attestation subnet bitfield
        let mut new_attestation_subnets = AttestationSubnets::new();
        for subnet_id in new_subnets {
            new_attestation_subnets.enable_attestation_subnet(subnet_id)?;
        }

        // Check if subnets actually changed
        if new_attestation_subnets == self.current_attestation_subnets {
            self.subscription_epoch = current_epoch;
            return Ok(false);
        }

        // Update ENR with new attestation subnets using enr_insert
        // The value needs to be RLP-encoded (via Encodable trait) for get_decodable to work
        let mut rlp_buffer = Vec::new();
        new_attestation_subnets.encode(&mut rlp_buffer);

        match self
            .discv5
            .enr_insert(ATTESTATION_BITFIELD_ENR_KEY, &rlp_buffer)
        {
            Ok(_) => {
                info!(
                    "Updated attestation subnet subscriptions at epoch {}: current subnets: {:?}",
                    current_epoch, new_attestation_subnets
                );
                self.current_attestation_subnets = new_attestation_subnets;
                self.subscription_epoch = current_epoch;
                Ok(true)
            }
            Err(err) => {
                error!("Failed to update local ENR with new attestation subnets: {err:?}");
                Err(anyhow!("Failed to update local ENR: {err:?}"))
            }
        }
    }

    /// Get the current attestation subnet subscriptions
    pub fn current_attestation_subnets(&self) -> &AttestationSubnets {
        &self.current_attestation_subnets
    }

    /// Get the epoch at which the current subscriptions were set
    pub fn subscription_epoch(&self) -> u64 {
        self.subscription_epoch
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
                    QueryType::Peers => {
                        let Some(Ok(fork_id)) = self
                            .discv5
                            .local_enr()
                            .get_decodable::<EnrForkId>(ENR_ETH2_KEY)
                        else {
                            warn!("ENR missing or invalid ENR_ETH2_KEY, skipping peer query");
                            return;
                        };
                        let fork_digest = fork_id.fork_digest;

                        Box::new(move |enr: &Enr| {
                            enr.get_decodable::<EnrForkId>(ENR_ETH2_KEY)
                                .and_then(Result::ok)
                                .map(|id| id.fork_digest == fork_digest)
                                .unwrap_or(false)
                                && (enr.tcp4().is_some() || enr.tcp6().is_some())
                        })
                    }
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
                        Err(err) => {
                            warn!("Failed to find peers: {err:?}");
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

    pub fn local_enr(&self) -> Enr {
        self.discv5.local_enr()
    }
}

impl NetworkBehaviour for Discovery {
    type ConnectionHandler = ConnectionHandler;
    type ToSwarm = DiscoveryOutEvent;

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
        trace!("Discv5 on swarm event gotten: {event:?}");
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
            return Poll::Ready(ToSwarm::GenerateEvent(DiscoveryOutEvent::DiscoveredPeers {
                peers,
            }));
        }

        match &mut self.event_stream {
            EventStream::Inactive => {}
            EventStream::Awaiting(fut) => {
                if let Poll::Ready(event_stream) = fut.poll_unpin(cx) {
                    match event_stream {
                        Ok(stream) => {
                            self.event_stream = EventStream::Present(stream);
                        }
                        Err(err) => {
                            error!("Failed to start discovery event stream: {err:?}");
                            self.event_stream = EventStream::Inactive;
                        }
                    }
                }
            }
            EventStream::Present(receiver) => match receiver.try_recv() {
                Ok(event) => {
                    if let Event::SocketUpdated(_) = event {
                        return Poll::Ready(ToSwarm::GenerateEvent(
                            DiscoveryOutEvent::UpdatedEnr {
                                enr: self.local_enr(),
                            },
                        ));
                    }
                }
                Err(err) => {
                    warn!("No discovery event found: {err:?}");
                    self.event_stream = EventStream::Inactive;
                }
            },
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
    use ream_consensus_misc::constants::beacon::GENESIS_VALIDATORS_ROOT;
    use ream_network_spec::networks::initialize_test_network_spec;

    use super::*;
    use crate::{
        config::DiscoveryConfig,
        subnet::{AttestationSubnets, EPOCHS_PER_SUBNET_SUBSCRIPTION, SyncCommitteeSubnets},
    };

    #[tokio::test]
    async fn test_initial_subnet_setup() -> anyhow::Result<()> {
        let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
        initialize_test_network_spec();
        let key = Keypair::generate_secp256k1();
        let mut config = DiscoveryConfig {
            disable_discovery: true,
            ..DiscoveryConfig::default()
        };
        config.attestation_subnets.enable_attestation_subnet(0)?; // Set subnet 0
        config.attestation_subnets.disable_attestation_subnet(1)?; // Set subnet 1

        let discovery = Discovery::new(key, &config, 0).await.unwrap();
        // Check ENR reflects config.subnets
        let enr_subnets = discovery
            .discv5
            .local_enr()
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .ok_or("ATTESTATION_BITFIELD_ENR_KEY not found")
            .map_err(|err| anyhow!("ATTESTATION_BITFIELD_ENR_KEY decoding failed: {err:?}"))??;
        assert!(enr_subnets.is_attestation_subnet_enabled(0)?);
        assert!(!enr_subnets.is_attestation_subnet_enabled(1)?);
        Ok(())
    }

    #[tokio::test]
    async fn test_attestation_subnet_predicate() -> anyhow::Result<()> {
        initialize_test_network_spec();
        let key = Keypair::generate_secp256k1();
        let mut config = DiscoveryConfig::default();
        config.attestation_subnets.enable_attestation_subnet(0)?; // Local node on subnet 0
        config.attestation_subnets.disable_attestation_subnet(1)?;
        config.disable_discovery = true;

        let discovery = Discovery::new(key, &config, 0).await.unwrap();
        let local_enr = discovery.local_enr();

        // Predicate for subnet 0 should match
        let predicate = attestation_subnet_predicate(vec![0]);
        assert!(predicate(&local_enr));

        // Predicate for subnet 1 should not match
        let predicate = attestation_subnet_predicate(vec![1]);
        assert!(!predicate(&local_enr));
        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_with_subnets() -> anyhow::Result<()> {
        initialize_test_network_spec();
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
        let mut discovery = Discovery::new(key, &config, 0).await.unwrap();

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

        let peer_discovery = Discovery::new(peer_key, &peer_config, 0).await.unwrap();
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
        if let Poll::Ready(ToSwarm::GenerateEvent(DiscoveryOutEvent::DiscoveredPeers { peers })) =
            discovery.poll(&mut cx)
        {
            assert_eq!(peers.len(), 1);
            assert!(peers.contains_key(&peer_discovery.local_enr()));
        } else {
            panic!("Expected peers to be discovered");
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_subnet_subscription_update() -> anyhow::Result<()> {
        let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
        initialize_test_network_spec();

        let key = Keypair::generate_secp256k1();
        let config = DiscoveryConfig {
            disable_discovery: true,
            ..DiscoveryConfig::default()
        };

        // Start at epoch 0
        let initial_slot = 0;
        let mut discovery = Discovery::new(key, &config, initial_slot).await?;

        // Get initial subscriptions
        let initial_subnets = discovery.current_attestation_subnets().clone();
        let _initial_epoch = discovery.subscription_epoch();

        assert_eq!(_initial_epoch, 0);

        // Try to update with a slot in the same epoch - should not change
        let same_epoch_slot = 10;
        let _updated = discovery.update_attestation_subnets(same_epoch_slot)?;
        assert!(
            !_updated,
            "Should not update subnets within the same subscription period"
        );
        assert_eq!(discovery.current_attestation_subnets(), &initial_subnets);

        // Move forward to a slot that should trigger rotation (256 epochs later)
        let rotation_slot = EPOCHS_PER_SUBNET_SUBSCRIPTION * 32; // Each epoch has 32 slots
        let _updated = discovery.update_attestation_subnets(rotation_slot)?;

        // The subnets may or may not change depending on the hash, but the epoch should update
        assert_eq!(
            discovery.subscription_epoch(),
            EPOCHS_PER_SUBNET_SUBSCRIPTION
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_subnet_subscription_determinism() -> anyhow::Result<()> {
        let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
        initialize_test_network_spec();

        let key = Keypair::generate_secp256k1();
        let config = DiscoveryConfig {
            disable_discovery: true,
            ..DiscoveryConfig::default()
        };

        let initial_slot = 0;
        let discovery = Discovery::new(key.clone(), &config, initial_slot).await?;
        let subnets1 = discovery.current_attestation_subnets().clone();

        // Create another discovery instance with the same key
        let discovery2 = Discovery::new(key, &config, initial_slot).await?;
        let subnets2 = discovery2.current_attestation_subnets().clone();

        // Should have the same subnets since they have the same node_id
        assert_eq!(
            subnets1, subnets2,
            "Subnet subscriptions should be deterministic"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_subnet_subscription_rotation() -> anyhow::Result<()> {
        let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
        initialize_test_network_spec();

        let key = Keypair::generate_secp256k1();
        let config = DiscoveryConfig {
            disable_discovery: true,
            ..DiscoveryConfig::default()
        };

        let initial_slot = 0;
        let mut discovery = Discovery::new(key, &config, initial_slot).await?;

        let initial_subnets = discovery.current_attestation_subnets().clone();
        let _initial_epoch = discovery.subscription_epoch();

        // Move forward multiple subscription periods
        for i in 1..=3 {
            let rotation_slot = i * EPOCHS_PER_SUBNET_SUBSCRIPTION * 32;
            let _ = discovery.update_attestation_subnets(rotation_slot)?;

            // Epoch should update
            assert_eq!(
                discovery.subscription_epoch(),
                i * EPOCHS_PER_SUBNET_SUBSCRIPTION,
                "Subscription epoch should update after rotation"
            );
        }

        // After going through multiple periods, we should eventually see different subnets
        // (This is probabilistic, but with 64 subnets and 2 subscriptions, it's very likely)
        info!(
            "Initial subnets: {:?}, Final subnets: {:?}",
            initial_subnets,
            discovery.current_attestation_subnets()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_subnet_enr_update() -> anyhow::Result<()> {
        let _ = GENESIS_VALIDATORS_ROOT.set(B256::ZERO);
        initialize_test_network_spec();

        let key = Keypair::generate_secp256k1();
        let config = DiscoveryConfig {
            disable_discovery: true,
            ..DiscoveryConfig::default()
        };

        let initial_slot = 0;
        let mut discovery = Discovery::new(key, &config, initial_slot).await?;

        // Get initial ENR subnets
        let initial_enr_subnets = discovery
            .local_enr()
            .get_decodable::<AttestationSubnets>(ATTESTATION_BITFIELD_ENR_KEY)
            .ok_or_else(|| anyhow!("Missing attestation subnet field"))?
            .map_err(|err| anyhow!("Failed to decode attestation subnets: {err:?}"))?;

        // Trigger a rotation
        let rotation_slot = EPOCHS_PER_SUBNET_SUBSCRIPTION * 32;
        let _updated = discovery.update_attestation_subnets(rotation_slot)?;

        // Internal state should always be up to date
        assert_eq!(
            discovery.subscription_epoch(),
            EPOCHS_PER_SUBNET_SUBSCRIPTION,
            "Subscription epoch should be updated"
        );

        // The current_attestation_subnets field should reflect the new subscriptions
        info!(
            "Initial subnets: {:?}, Current subnets: {:?}",
            initial_enr_subnets,
            discovery.current_attestation_subnets()
        );

        Ok(())
    }
}
