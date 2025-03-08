use ream_discv5::config::DiscoveryConfig;
use ream_gossipsub::{config::GossipsubConfig, topics::GossipTopic};

pub struct NetworkConfig {
    pub disc_config: DiscoveryConfig,

    pub gossipsub_config: GossipsubConfig,

    pub topics: Vec<GossipTopic>,
}
