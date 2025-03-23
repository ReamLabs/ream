use std::time::Duration;

use libp2p::gossipsub::{self};

use crate::topics::{GossipTopic, GossipTopicKind};

#[derive(Debug, Clone)]
pub struct GossipsubConfig {
    pub config: gossipsub::Config,
    pub topics: Vec<GossipTopic>,
}

impl GossipsubConfig {
    // https://ethereum.github.io/consensus-specs/specs/phase0/p2p-interface/#the-gossip-domain-gossipsub
    pub fn default() -> Self {
        let seconds_per_slot = 12;
        let slots_per_epoch = 32;
        let config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(1023 * 1024)
            .heartbeat_interval(Duration::from_millis(700))
            .fanout_ttl(Duration::from_secs(60))
            .mesh_n(8)
            .mesh_n_low(6)
            .mesh_n_high(12)
            .gossip_lazy(6)
            .history_length(6)
            .history_gossip(3)
            .duplicate_cache_time(Duration::from_secs(slots_per_epoch * seconds_per_slot * 2))
            .validate_messages()
            .validation_mode(gossipsub::ValidationMode::Anonymous)
            .build()
            .expect("Failed to build gossipsub config");

        Self {
            config,
            topics: vec![GossipTopic {
                fork: [106, 149, 161, 169], // 0x6a95a1a9 (deneb)
                kind: GossipTopicKind::BeaconBlock,
            }],
        }
    }
}
