use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use discv5::Enr;

use crate::subnet::{Subnet, Subnets};

pub struct NetworkConfig {
    pub discv5_config: discv5::Config,
    pub bootnodes: Vec<Enr>,
    pub socket_address: IpAddr,
    pub socket_port: u16,
    pub disable_discovery: bool,
    pub total_peers: usize,
    pub subnets: Subnets,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        let mut subnets = Subnets::new();
        // Enable attestation subnets 0 and 1 as a reasonable default
        let _ = subnets.enable_subnet(Subnet::Attestation(0));
        let _ = subnets.enable_subnet(Subnet::Attestation(1));

        let filter_rate_limiter = Some(
            discv5::RateLimiterBuilder::new()
                .total_n_every(10, Duration::from_secs(1)) // Allow bursts, average 10 per second
                .ip_n_every(9, Duration::from_secs(1)) // Allow bursts, average 9 per second
                .node_n_every(8, Duration::from_secs(1)) // Allow bursts, average 8 per second
                .build()
                .expect("The total rate limit has been specified"),
        );

        let discv5_listen_config =
            discv5::ListenConfig::from_ip(Ipv4Addr::UNSPECIFIED.into(), 9000);

        // discv5 configuration
        let discv5_config = discv5::ConfigBuilder::new(discv5_listen_config)
            .enable_packet_filter()
            .session_cache_capacity(5000)
            .request_timeout(Duration::from_secs(2))
            .query_peer_timeout(Duration::from_secs(2))
            .query_timeout(Duration::from_secs(30))
            .request_retries(1)
            .enr_peer_update_min(10)
            .query_parallelism(8)
            .disable_report_discovered_peers()
            .ip_limit() // limits /24 IP's in buckets.
            .incoming_bucket_limit(8) // half the bucket size
            .filter_rate_limiter(filter_rate_limiter)
            .filter_max_bans_per_ip(Some(5))
            .filter_max_nodes_per_ip(Some(10))
            .ban_duration(Some(Duration::from_secs(3600)))
            .ping_interval(Duration::from_secs(300))
            .build();

        Self {
            discv5_config,
            bootnodes: Vec::new(),
            socket_address: IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
            socket_port: 9000,
            disable_discovery: false,
            total_peers: 10,
            subnets,
        }
    }
}
