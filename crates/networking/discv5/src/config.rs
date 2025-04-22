use std::net::{IpAddr, Ipv4Addr};

use discv5::{ConfigBuilder, Enr, ListenConfig};
use rand::{Rng, rngs::ThreadRng};

use crate::subnet::{Subnet, Subnets};

pub const SYNC_COMMITTEE_SUBNET_COUNT: usize = 4;

pub struct NetworkConfig {
    pub discv5_config: discv5::Config,
    pub bootnodes: Vec<Enr>,
    pub socket_address: IpAddr,
    pub socket_port: u16,
    pub discovery_port: u16,
    pub disable_discovery: bool,
    pub subnets: Subnets,
}

impl NetworkConfig {
    /// Subscribe to a sync committee subnet and update the ENR
    pub fn subscribe_to_sync_committee_subnet(&mut self, subnet_id: u8) -> anyhow::Result<()> {
        self.subnets.enable_subnet(Subnet::SyncCommittee(subnet_id))
    }

    /// Unsubscribe from a sync committee subnet and update the ENR
    pub fn unsubscribe_from_sync_committee_subnet(&mut self, subnet_id: u8) -> anyhow::Result<()> {
        self.subnets
            .disable_subnet(Subnet::SyncCommittee(subnet_id))
    }

    /// Calculate when to join a sync committee subnet based on the spec
    ///
    /// Returns the number of epochs before the next sync committee period to join the subnet
    pub fn calculate_sync_subnet_join_epoch_offset(&self) -> u64 {
        // Per spec: select a random number of epochs before the end of the current sync committee
        // period between 1 and SYNC_COMMITTEE_SUBNET_COUNT, inclusive.
        let mut rng = ThreadRng::default();
        rng.gen_range(1..=SYNC_COMMITTEE_SUBNET_COUNT as u64)
    }

    /// Calculate the epoch when we should join the sync committee subnet
    ///
    /// Takes the current epoch and the next sync committee period start epoch
    pub fn calculate_sync_subnet_join_epoch(
        &self,
        current_epoch: u64,
        next_sync_committee_period_start_epoch: u64,
    ) -> u64 {
        let offset = self.calculate_sync_subnet_join_epoch_offset();

        // If the next period is too close, we join immediately
        if next_sync_committee_period_start_epoch <= current_epoch + offset {
            current_epoch
        } else {
            next_sync_committee_period_start_epoch - offset
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        let mut subnets = Subnets::new();

        // Enable attestation subnets 0 and 1 as a reasonable default
        subnets.enable_subnet(Subnet::Attestation(0)).expect("xyz");
        subnets.enable_subnet(Subnet::Attestation(1)).expect("xyz");

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
            subnets,
        }
    }
}
