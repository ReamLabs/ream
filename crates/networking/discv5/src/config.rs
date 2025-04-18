use std::net::{IpAddr, Ipv4Addr};

use discv5::{ConfigBuilder, Enr, ListenConfig};

use crate::subnet::{Subnet, Subnets};

pub struct NetworkConfig {
    pub discv5_config: discv5::Config,
    pub bootnodes: Vec<Enr>,
    pub socket_address: Option<IpAddr>,
    pub socket_port: Option<u16>,
    pub disable_discovery: bool,
    pub subnets: Subnets,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        let mut subnets = Subnets::new();
        // Enable attestation subnets 0 and 1 as a reasonable default
        subnets.enable_subnet(Subnet::Attestation(0)).expect("xyz");
        subnets.enable_subnet(Subnet::Attestation(1)).expect("xyz");

        let socket_address = Ipv4Addr::UNSPECIFIED;
        let socket_port = 9000;
        let listen_config = ListenConfig::from_ip(socket_address.into(), socket_port);

        let discv5_config = ConfigBuilder::new(listen_config).build();

        Self {
            discv5_config,
            bootnodes: Vec::new(),
            socket_address: Some(socket_address.into()),
            socket_port: Some(socket_port),
            disable_discovery: false,
            subnets,
        }
    }
}
