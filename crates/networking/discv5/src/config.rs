use std::net::{IpAddr, Ipv4Addr};

use discv5::{ConfigBuilder, Enr, ListenConfig};

use crate::subnet::{AttestationSubnets, SyncCommitteeSubnets};

pub struct DiscoveryConfig {
    pub discv5_config: discv5::Config,
    pub bootnodes: Vec<Enr>,
    pub socket_address: IpAddr,
    pub socket_port: u16,
    pub discovery_port: u16,
    pub disable_discovery: bool,
    pub attestation_subnets: AttestationSubnets,
    pub sync_committee_subnets: SyncCommitteeSubnets,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        let mut attestation_subnets = AttestationSubnets::new();
        let sync_committee_subnets = SyncCommitteeSubnets::new();

        // Enable attestation subnets 0 and 1 as a reasonable default
        attestation_subnets
            .enable_attestation_subnet(0)
            .expect("Failed to enable attestation subnet 0");
        attestation_subnets
            .enable_attestation_subnet(1)
            .expect("Failed to enable attestation subnet 1");

        let socket_address = Ipv4Addr::UNSPECIFIED;
        let socket_port = 9000;
        let discovery_port = 9000;
        let listen_config = ListenConfig::from_ip(socket_address.into(), discovery_port);

        let discv5_config = ConfigBuilder::new(listen_config).build();

        Self {
            discv5_config,
            bootnodes: Vec::new(),
            socket_address: socket_address.into(),
            socket_port,
            discovery_port,
            disable_discovery: false,
            attestation_subnets,
            sync_committee_subnets,
        }
    }
}
