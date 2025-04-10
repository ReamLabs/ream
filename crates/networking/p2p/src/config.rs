use std::net::IpAddr;

use ream_discv5::config::DiscoveryConfig;
use ream_gossipsub::config::GossipsubConfig;

pub struct NetworkConfig {
    pub socket_address: IpAddr,

    pub socket_port: u16,

    pub disc_config: DiscoveryConfig,

    pub gossipsub_config: GossipsubConfig,
}
