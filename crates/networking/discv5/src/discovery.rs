use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use discv5::{enr::CombinedKey, Discv5, Enr};
use futures::{stream::FuturesUnordered, StreamExt, TryFutureExt};
use libp2p::{
    core::{transport::PortUse, Endpoint},
    identity::Keypair,
    swarm::{
        dummy::ConnectionHandler, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour,
        THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};

use crate::config::NetworkConfig;

#[derive(Debug)]
pub struct DiscoveredPeers {
    pub _peers: HashMap<Enr, Option<Instant>>,
}

enum EventStream {
    Inactive,
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
    pub async fn new(local_key: Keypair, config: &NetworkConfig) -> Result<Self, String> {
        let enr_local = convert_to_enr(local_key)?;
        let enr = Enr::builder().build(&enr_local).unwrap();
        let node_local_id = enr.node_id();

        let mut discv5 = Discv5::new(enr, enr_local, config.discv5_config.clone())
            .map_err(|e| format!("Discv5 service failed. Error: {:?}", e))?;

        // adding bootnode to DHT
        for bootnode_enr in config.boot_nodes_enr.clone() {
            if bootnode_enr.node_id() == node_local_id {
                // Skip adding ourselves to the routing table if we are a bootnode
                continue;
            }

            let _ = discv5.add_enr(bootnode_enr).map_err(|e| {
                println!("Discv5 service failed. Error: {:?}", e);
            });
        }

        // init ports
        let event_stream = if !config.disable_discovery {
            discv5.start().map_err(|e| e.to_string()).await?;
            println!("Started discovery");
            // EventStream::Awaiting(Box::pin(discv5.event_stream()))
            EventStream::Inactive
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
        println!("Discovering peers {:?}", self.discv5.local_enr());

        if !self.started || self.find_peer_active {
            return;
        }

        self.find_peer_active = true;
        self.start_query(QueryType::FindPeers, target_peers);
    }

    fn process_queries(&mut self, cx: &mut Context) -> bool {
        let mut processed = false;

        while let Poll::Ready(Some(query)) = self.discovery_queries.poll_next_unpin(cx) {
            println!("query{:?} {:?}", query.result, query.query_type);
            processed = true;
            // TODO: add query types and push them to mesh
        }
        processed
    }

    fn start_query(&mut self, query: QueryType, _total_peers: usize) {
        println!("Query! queryType={:?}", query);
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

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        Ok(Vec::new())
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
        println!("Swarm event: {:?}", event);
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
        println!("ConnectionHandlerOutEvent");
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.process_queries(cx);

        match self.event_stream {
            EventStream::Inactive => println!("inactive"),
        };

        Poll::Pending
    }
}

fn convert_to_enr(key: Keypair) -> Result<CombinedKey, &'static str> {
    let key = key.try_into_secp256k1().expect("right key type");
    let secret = discv5::enr::k256::ecdsa::SigningKey::from_slice(&key.secret().to_bytes())
        .expect("libp2p key must be valid");
    Ok(CombinedKey::Secp256k1(secret))
}
