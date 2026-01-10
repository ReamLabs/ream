use std::time::Duration;

use libp2p::gossipsub::{PeerScoreParams, TopicHash, TopicScoreParams};
use ream_network_spec::networks::beacon_network_spec;

use super::{EPOCH_DURATION_SLOTS, topic_params::build_topic_params};
use crate::gossipsub::{
    beacon::topics::{GossipTopic, GossipTopicKind},
    common::scoring,
};

/// Build Ethereum spec-compliant PeerScoreParams.
///
/// This configures the global scoring parameters and topic-specific parameters
/// as defined in the Ethereum P2P specification.
///
/// See: https://ethereum.github.io/consensus-specs/specs/phase0/p2p-interface/#peer-scoring
pub fn build_peer_score_params(topics: &[GossipTopic]) -> PeerScoreParams {
    let network_spec = beacon_network_spec();
    let slot_duration = Duration::from_secs(network_spec.seconds_per_slot);

    scoring::build_peer_score_params(
        slot_duration,
        EPOCH_DURATION_SLOTS,
        |params, _epoch_slots| {
            // Add beacon-specific topic scoring
            for topic in topics {
                let topic_hash: TopicHash = (*topic).into();
                let topic_params = build_topic_params(&topic.kind);
                scoring::add_topic(params, topic_hash, topic_params);
            }
        },
    )
}

/// Build PeerScoreParams with topic hashes directly.
/// Useful when topics are already known as TopicHashes.
pub fn build_peer_score_params_with_topic_kinds(
    topic_hashes: &[(TopicHash, GossipTopicKind)],
) -> PeerScoreParams {
    let network_spec = beacon_network_spec();
    let slot_duration = Duration::from_secs(network_spec.seconds_per_slot);

    scoring::build_peer_score_params(
        slot_duration,
        EPOCH_DURATION_SLOTS,
        |params, _epoch_slots| {
            for (topic_hash, kind) in topic_hashes {
                let topic_params = build_topic_params(kind);
                scoring::add_topic(params, topic_hash.clone(), topic_params);
            }
        },
    )
}

/// Build default topic params for topics not explicitly configured.
pub fn build_default_topic_params() -> TopicScoreParams {
    build_topic_params(&GossipTopicKind::VoluntaryExit)
}

#[cfg(test)]
mod tests {
    use ream_network_spec::networks::beacon::initialize_test_network_spec;

    use super::*;

    #[test]
    fn test_build_peer_score_params() {
        initialize_test_network_spec();

        let topics = vec![];
        let params = build_peer_score_params(&topics);

        // Verify basic parameters are set correctly
        assert!(params.decay_to_zero > 0.0);
        assert!(params.ip_colocation_factor_weight < 0.0);
        assert!(params.behaviour_penalty_weight < 0.0);
    }
}
