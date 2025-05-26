use std::net::IpAddr;

use ream_discv5::config::DiscoveryConfig;

use crate::gossipsub::configurations::GossipsubConfig;

pub struct NetworkConfig {
    pub socket_address: IpAddr,

    pub socket_port: u16,

    pub discv5_config: DiscoveryConfig,

    pub gossipsub_config: GossipsubConfig,
}
