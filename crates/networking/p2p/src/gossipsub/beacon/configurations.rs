use std::time::Duration;

use libp2p::gossipsub::{
    Config, ConfigBuilder, MessageId, PeerScoreParams, PeerScoreThresholds, ValidationMode,
};
use ream_consensus_misc::constants::beacon::SLOTS_PER_EPOCH;
use ream_network_spec::networks::beacon_network_spec;
use sha2::{Digest, Sha256};

use super::{scoring::params::build_peer_score_params, topics::GossipTopic};
use crate::{
    constants::MESSAGE_DOMAIN_VALID_SNAPPY,
    gossipsub::common::scoring::build_peer_score_thresholds, utils::max_message_size,
};

#[derive(Debug, Clone)]
pub struct GossipsubConfig {
    pub config: Config,
    pub topics: Vec<GossipTopic>,
    pub peer_score_params: PeerScoreParams,
    pub peer_score_thresholds: PeerScoreThresholds,
}

impl Default for GossipsubConfig {
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

        // Initialize with empty topics - will be populated via set_topics()
        let topics = vec![];
        let peer_score_params = build_peer_score_params(&topics);
        let peer_score_thresholds = build_peer_score_thresholds();

        Self {
            config,
            topics,
            peer_score_params,
            peer_score_thresholds,
        }
    }
}

impl GossipsubConfig {
    /// Set the topics and rebuild topic-specific scoring parameters.
    pub fn set_topics(&mut self, topics: Vec<GossipTopic>) {
        self.topics = topics.clone();
        self.peer_score_params = build_peer_score_params(&topics);
    }
}

#[cfg(test)]
mod test {
    use ream_network_spec::networks::beacon::initialize_test_network_spec;

    use crate::gossipsub::{
        beacon::configurations::GossipsubConfig,
        configurations::{
            assert_common, consistant_message_id_caching, message_collisions,
            message_instantiation_edge_cases, valid_message_id_computation,
        },
    };

    #[test]
    fn test_gossipsub_parameters() {
        initialize_test_network_spec();

        let gossipsub_config = GossipsubConfig::default();
        let config = &gossipsub_config.config;

        assert_common(config);
    }

    #[test]
    fn test_message_id_computation() {
        initialize_test_network_spec();
        let config = &GossipsubConfig::default().config;

        valid_message_id_computation(config);
    }

    #[test]
    fn test_message_id_caching() {
        initialize_test_network_spec();
        let config = &GossipsubConfig::default().config;

        consistant_message_id_caching(config);
    }

    #[test]
    fn test_message_id_edge_cases() {
        initialize_test_network_spec();
        let config = &GossipsubConfig::default().config;

        message_instantiation_edge_cases(config);
    }

    #[test]
    fn test_message_uniqueness_and_collision_resistance() {
        initialize_test_network_spec();
        let config = &GossipsubConfig::default().config;

        message_collisions(config);
    }
}
