use std::time::Duration;

use libp2p::gossipsub::{Config, ConfigBuilder, MessageId, ValidationMode};
use ream_consensus_misc::constants::beacon::SLOTS_PER_EPOCH;
use ream_network_spec::networks::beacon_network_spec;
use sha2::{Digest, Sha256};

use crate::{
    constants::MESSAGE_DOMAIN_VALID_SNAPPY, gossipsub::lean::topics::LeanGossipTopic,
    utils::max_message_size,
};

#[derive(Debug, Clone)]
pub struct LeanGossipsubConfig {
    pub config: Config,
    pub topics: Vec<LeanGossipTopic>,
}

impl Default for LeanGossipsubConfig {
    // https://ethereum.github.io/consensus-specs/specs/phase0/p2p-interface/#the-gossip-domain-gossipsub
    fn default() -> Self {
        let config = ConfigBuilder::default()
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
            .duplicate_cache_time(Duration::from_secs(
                SLOTS_PER_EPOCH * beacon_network_spec().seconds_per_slot * 2,
            ))
            .validate_messages()
            .validation_mode(ValidationMode::Anonymous)
            .allow_self_origin(true)
            .flood_publish(false)
            .idontwant_message_size_threshold(1000)
            .message_id_fn(move |message| {
                MessageId::from(
                    &Sha256::digest({
                        let topic_bytes = message.topic.as_str().as_bytes();
                        let mut digest = vec![];
                        digest.extend_from_slice(MESSAGE_DOMAIN_VALID_SNAPPY.as_slice());
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

impl LeanGossipsubConfig {
    pub fn set_topics(&mut self, topics: Vec<LeanGossipTopic>) {
        self.topics = topics;
    }
}
