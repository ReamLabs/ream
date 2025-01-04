use std::{future::Future, pin::Pin, sync::mpsc, time::Instant};

use discv5::{Discv5, Enr};
use discv5::enr::CombinedKey;
use futures::stream::FuturesUnordered;
use futures::TryFutureExt;
use libp2p::identity::Keypair;
use log::error;
use crate::config::NetworkConfig;

enum EventStream {
    Awaiting(
        Pin<Box<dyn Future<Output = Result<mpsc::Receiver<discv5::Event>, discv5::Error>> + Send>>,
    ),
    Present(mpsc::Receiver<discv5::Event>),
    Inactive,
}

#[derive(Debug, Clone, PartialEq)]
struct SubnetQuery {
    min_ttl: Option<Instant>,
    retries: usize,
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

    pub async fn new(
        &mut self,
        local_key: Keypair
        config: &NetworkConfig
    ) -> Result<Self,String>{
        let enr_local = self.convert_to_enr(local_key)?;
        let enr = discv5::enr::Enr::builder().build(&enr_local).unwrap();
        let node_local_id = enr.node_id()

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

        Ok(Self{
            discv5
            event_stream,
            discovery_queries
            find_peer_active: false
            started: true,
        })


    }
    fn convert_to_enr(&self, key: Keypair)->Result<CombinedKey, &'static str> {
        let key = key.try_into_secp256k1().expect("right key type");
        let secret =
            discv5::enr::k256::ecdsa::SigningKey::from_slice(&key.secret().to_bytes())
                .expect("libp2p key must be valid");
        Ok(CombinedKey::Secp256k1(secret))
    }



}
