use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use anyhow::anyhow;
use discv5::{
    enr::{k256::ecdsa::SigningKey, CombinedKey, NodeId},
    Discv5, Enr,
};
use futures::{stream::FuturesUnordered, FutureExt, StreamExt, TryFutureExt};
use libp2p::{
    core::{transport::PortUse, Endpoint},
    identity::Keypair,
    swarm::{
        dummy::ConnectionHandler, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour,
        THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::NetworkConfig;

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
}

impl Discovery {
    pub async fn new(local_key: Keypair, config: &NetworkConfig) -> anyhow::Result<Self> {
        let enr_local = convert_to_enr(local_key)?;
        let enr = Enr::builder().build(&enr_local).unwrap();
        let node_local_id = enr.node_id();

        let mut discv5 = Discv5::new(enr, enr_local, config.discv5_config.clone())
            .map_err(|err| anyhow!("Failed to create discv5: {err:?}"))?;

        // adding bootnode to DHT
        for bootnode_enr in config.boot_nodes_enr.clone() {
            if bootnode_enr.node_id() == node_local_id {
                // Skip adding ourselves to the routing table if we are a bootnode
                continue;
            }

            let _ = discv5.add_enr(bootnode_enr).map_err(|err| {
                error!("Failed to add bootnode to DHT {err:?}");
            });
        }

        // init ports
        let event_stream = if !config.disable_discovery {
            discv5
                .start()
                .map_err(|err| anyhow!("Failed to start discv5 {err:?}"))
                .await?;
            info!("Started discovery");
            EventStream::Awaiting(Box::pin(discv5.event_stream()))
        } else {
            EventStream::Inactive
        };

        Ok(Self {
            discv5,
            event_stream,
            discovery_queries: FuturesUnordered::new(),
            find_peer_active: false,
            started: true,
        })
    }

    pub fn discover_peers(&mut self, target_peers: usize) {
        // If the discv5 service isn't running or we are in the process of a query, don't bother
        // queuing a new one.
        info!("Discovering peers {:?}", self.discv5.local_enr());

        if !self.started || self.find_peer_active {
            return;
        }

        self.find_peer_active = true;
        self.start_query(QueryType::FindPeers, target_peers);
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
            };

            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn start_query(&mut self, query: QueryType, _total_peers: usize) {
        info!("Query! queryType={:?}", query);
        let query_future = self
            .discv5
            .find_node(NodeId::random())
            .map(|result| QueryResult {
                query_type: query,
                result,
            });

        self.discovery_queries.push(Box::pin(query_future));
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
