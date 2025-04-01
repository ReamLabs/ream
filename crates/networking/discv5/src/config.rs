use std::net::IpAddr;

use discv5::Enr;

pub struct NetworkConfig {
    pub discv5_config: discv5::Config,

    pub bootnodes: Vec<Enr>,

    pub socket_address: IpAddr,

    pub socket_port: u16,

    pub disable_discovery: bool,

    pub total_peers: usize,
}
