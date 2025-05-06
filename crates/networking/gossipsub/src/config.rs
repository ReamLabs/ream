use std::{cmp::max, time::Duration};

use libp2p::gossipsub::{self};
use sha2::{Digest, Sha256};

use crate::topics::GossipTopic;

pub const SECONDS_PER_SLOT: u64 = 12;
pub const SLOTS_PER_EPOCH: u64 = 32;
pub const MAX_PAYLOAD_SIZE: u64 = 10485760;
pub const MESSAGE_DOMAIN_VALID_SNAPPY: [u8; 4] = [0x01, 0x00, 0x00, 0x00];
pub const MESSAGE_DOMAIN_INVALID_SNAPPY: [u8; 4] = [0x00, 0x00, 0x00, 0x00];

#[derive(Debug, Clone)]
pub struct GossipsubConfig {
    pub config: gossipsub::Config,
    pub topics: Vec<GossipTopic>,
}

impl Default for GossipsubConfig {
    // https://ethereum.github.io/consensus-specs/specs/phase0/p2p-interface/#the-gossip-domain-gossipsub
    fn default() -> Self {
        let config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(max_message_size() as usize)
            .heartbeat_interval(Duration::from_millis(700))
            .fanout_ttl(Duration::from_secs(60))
            .mesh_n(8)
            .mesh_n_low(6)
            .mesh_n_high(12)
            .gossip_lazy(6)
            .history_length(12)
            .history_gossip(3)
            .max_messages_per_rpc(Some(500))
            .duplicate_cache_time(Duration::from_secs(SLOTS_PER_EPOCH * SECONDS_PER_SLOT * 2))
            .validate_messages()
            .validation_mode(gossipsub::ValidationMode::Anonymous)
            .allow_self_origin(true)
            .flood_publish(false)
            .idontwant_message_size_threshold(1000)
            .message_id_fn(move |message| {
                gossipsub::MessageId::from(
                    &Sha256::digest({
                        let topic_bytes = message.topic.as_str().as_bytes();
                        let mut digest = vec![];
                        digest.extend_from_slice(&MESSAGE_DOMAIN_VALID_SNAPPY);
                        digest.extend_from_slice(&topic_bytes.len().to_le_bytes());
                        digest.extend_from_slice(topic_bytes);
                        digest.extend_from_slice(&message.data);
                        digest
                    })[..20],
                )
            })
            .build()
            .expect("Failed to build gossipsub config");

        Self {
            config,
            topics: vec![],
        }
    }
}

impl GossipsubConfig {
    pub fn set_topics(&mut self, topics: Vec<GossipTopic>) {
        self.topics = topics;
    }
}

fn max_compressed_len(n: u64) -> u64 {
    32 + n + n / 6
}

fn max_message_size() -> u64 {
    max(max_compressed_len(MAX_PAYLOAD_SIZE) + 1024, 1024 * 1024)
}
