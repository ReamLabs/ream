use ream_discv5::config::DiscoveryConfig;
use ream_gossipsub::config::GossipsubConfig;

pub struct NetworkConfig {
    pub disc_config: DiscoveryConfig,

    pub gossipsub_config: GossipsubConfig,
}
