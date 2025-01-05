use std::{collections::HashMap, future::Future, pin::Pin, sync::mpsc, time::Instant};

use discv5::{
    enr::{CombinedKey, NodeId},
    Discv5, Enr,
};
use futures::{stream::FuturesUnordered, TryFutureExt};
use libp2p::identity::Keypair;

use crate::config::NetworkConfig;

#[derive(Debug)]
pub struct DiscoveredPeers {
    pub peers: HashMap<Enr, Option<Instant>>,
}

enum EventStream {
    Awaiting(
        Pin<Box<dyn Future<Output = Result<mpsc::Receiver<discv5::Event>, discv5::Error>> + Send>>,
    ),
    Present(mpsc::Receiver<discv5::Event>),
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
        let enr = discv5::enr::Enr::builder().build(&enr_local).unwrap();
        let node_local_id = enr.node_id();

        let mut discv5 = Discv5::new(enr, enr_local, config.discv5_config.clone())
            .map_err(|e| format!("Discv5 service failed. Error: {:?}", e))?;

        // adding bootnode to DHT
        for bootnode_enr in config.boot_nodes_enr.clone() {
            if bootnode_enr.node_id() == node_local_id {
                // If we are a boot node, ignore adding it to the routing table
                continue;
            }

            let repr = bootnode_enr.to_string();
            let _ = discv5.add_enr(bootnode_enr).map_err(|e| {
                println!("Discv5 service failed. Error: {:?}", e);
            });
        }

        // init ports
        let event_stream = if !config.disable_discovery {
            discv5.start().map_err(|e| e.to_string()).await?;
            println!("Started discovery");
            EventStream::Awaiting(Box::pin(discv5.event_stream()))
        } else {
            EventStream::Inactive
        };

        Ok(Self {
            discv5,
            event_stream,
            discovery_queries,
            find_peer_active: false,
            started: true,
        })
    }

    pub fn discover_peers(&mut self, target_peers: usize) {
        // If the discv5 service isn't running or we are in the process of a query, don't bother
        // queuing a new one.
        if !self.started || self.find_peer_active {
            return;
        }
        // Immediately start a FindNode query

        self.find_peer_active = true;
        self.start_query(QueryType::FindPeers, target_peers);
    }

    fn process_queries(&mut self) -> bool {
        let mut processed = false;

        while &self.discovery_queries.is_empty() {
            // TODO: add query types and push them to mesh
        }
        processed
    }

    fn start_query(&mut self, query: QueryType, total_peers: usize) {
        let enr_fork_id = match self.local_enr().eth2() {
            Ok(v) => v,
            Err(e) => {
                println!(self.log, "Local ENR has no fork id"; "error" => e);
                return;
            }
        };

        let predicate = Box::new(|enr: &Enr| enr.ip().is_some());

        let query_future = self
            .discv5
            // Generate a random target node id.
            .find_node_predicate(NodeId::random(), total_peers, predicate)
            .map(|v| QueryResult {
                query_type: query,
                result: v,
            });

        self.discovery_queries.push(Box::pin(query_future));
    }
}

fn convert_to_enr(key: Keypair) -> Result<CombinedKey, &'static str> {
    let key = key.try_into_secp256k1().expect("right key type");
    let secret = discv5::enr::k256::ecdsa::SigningKey::from_slice(&key.secret().to_bytes())
        .expect("libp2p key must be valid");
    Ok(CombinedKey::Secp256k1(secret))
}
