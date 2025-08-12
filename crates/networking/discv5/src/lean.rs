use std::net::{IpAddr, Ipv4Addr};

use discv5::{ConfigBuilder, ListenConfig};

#[derive(Debug, Clone)]
pub struct LeanDiscoveryConfig {
    pub discv5_config: discv5::Config,
    pub socket_address: IpAddr,
    pub socket_port: u16,
    pub discovery_port: u16,
    pub disable_discovery: bool,
}

impl LeanDiscoveryConfig {
    pub fn new(socket_address: IpAddr, socket_port: u16, discovery_port: u16) -> Self {
        let listen_config = ListenConfig::from_ip(socket_address, discovery_port);
        let discv5_config = ConfigBuilder::new(listen_config).build();
        Self {
            discv5_config,
            socket_address,
            socket_port,
            discovery_port,
            disable_discovery: false,
        }
    }
}

impl Default for LeanDiscoveryConfig {
    fn default() -> Self {
        let socket_address = Ipv4Addr::UNSPECIFIED;
        let socket_port = 9000;
        let discovery_port = 9000;
        let listen_config = ListenConfig::from_ip(socket_address.into(), discovery_port);

        let discv5_config = ConfigBuilder::new(listen_config).build();

        Self {
            discv5_config,
            socket_address: socket_address.into(),
            socket_port,
            discovery_port,
            disable_discovery: false,
        }
    }
}
