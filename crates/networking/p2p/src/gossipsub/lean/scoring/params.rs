use std::time::Duration;

use libp2p::gossipsub::{PeerScoreParams, TopicHash};
use ream_network_spec::networks::lean_network_spec;

use super::{EPOCH_DURATION_SLOTS, topic_params::build_topic_params};
use crate::gossipsub::{
    common::scoring,
    lean::topics::{LeanGossipTopic, LeanGossipTopicKind},
};

/// Build lean network peer score params.
pub fn build_peer_score_params(topics: &[LeanGossipTopic]) -> PeerScoreParams {
    let network_spec = lean_network_spec();
    let slot_duration = Duration::from_secs(network_spec.seconds_per_slot);

    scoring::build_peer_score_params(
        slot_duration,
        EPOCH_DURATION_SLOTS,
        |params, _epoch_slots| {
            // Add lean-specific topic scoring
            for topic in topics {
                let topic_hash: TopicHash = topic.clone().into();
                let topic_params = build_topic_params(&topic.kind);
                scoring::add_topic(params, topic_hash, topic_params);
            }
        },
    )
}

/// Build PeerScoreParams with topic hashes directly.
pub fn build_peer_score_params_with_topic_kinds(
    topic_hashes: &[(TopicHash, LeanGossipTopicKind)],
) -> PeerScoreParams {
    let network_spec = lean_network_spec();
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

#[cfg(test)]
mod tests {
    use ream_network_spec::networks::lean::initialize_lean_test_network_spec;

    use super::*;

    #[test]
    fn test_build_peer_score_params() {
        initialize_lean_test_network_spec();

        let topics = vec![];
        let params = build_peer_score_params(&topics);

        // Verify basic parameters are set correctly
        assert!(params.decay_to_zero > 0.0);
        assert!(params.ip_colocation_factor_weight < 0.0);
        assert!(params.behaviour_penalty_weight < 0.0);
    }
}
