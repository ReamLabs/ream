use std::time::Duration;

use libp2p::gossipsub::{PeerScoreParams, TopicScoreParams};

use super::constants::{
    APP_SPECIFIC_WEIGHT, BEHAVIOUR_PENALTY_DECAY_EPOCHS, BEHAVIOUR_PENALTY_THRESHOLD,
    BEHAVIOUR_PENALTY_WEIGHT, DECAY_TO_ZERO, IP_COLOCATION_FACTOR_THRESHOLD,
    IP_COLOCATION_FACTOR_WEIGHT,
};

/// Compute decay factor for a given decay time in slots.
///
/// The decay factor is used to gradually reduce scores over time.
/// A score multiplied by this factor each decay interval will approach zero.
pub fn score_decay(decay_time_slots: f64) -> f64 {
    if decay_time_slots <= 0.0 {
        return 0.0;
    }
    // Score decays exponentially, halving roughly every decay_time_slots
    (-1.0 / decay_time_slots).exp()
}

/// Build generic peer score parameters with network-specific topic configurations.
///
/// This function creates the global scoring parameters and allows the caller
/// to add network-specific topic scoring via a closure.
#[allow(clippy::field_reassign_with_default)]
pub fn build_peer_score_params<F>(
    slot_duration: Duration,
    epoch_duration_slots: f64,
    add_topics: F,
) -> PeerScoreParams
where
    F: FnOnce(&mut PeerScoreParams, f64),
{
    let mut params = PeerScoreParams::default();

    // Decay interval: scores decay every slot
    params.decay_interval = slot_duration;

    // Score decays to zero when it falls below this threshold
    params.decay_to_zero = DECAY_TO_ZERO;

    // Application-specific scoring weight
    params.app_specific_weight = APP_SPECIFIC_WEIGHT;

    // IP colocation penalty: penalize peers sharing the same IP address
    params.ip_colocation_factor_weight = IP_COLOCATION_FACTOR_WEIGHT;
    params.ip_colocation_factor_threshold = IP_COLOCATION_FACTOR_THRESHOLD;

    // Behaviour penalty: penalize peers with bad behavior
    params.behaviour_penalty_weight = BEHAVIOUR_PENALTY_WEIGHT;
    params.behaviour_penalty_decay =
        score_decay(BEHAVIOUR_PENALTY_DECAY_EPOCHS * epoch_duration_slots);
    params.behaviour_penalty_threshold = BEHAVIOUR_PENALTY_THRESHOLD;

    // Add network-specific topic configurations
    add_topics(&mut params, epoch_duration_slots);

    params
}

/// Helper to add a topic with its scoring parameters to the peer score params.
pub fn add_topic<T>(params: &mut PeerScoreParams, topic: T, topic_params: TopicScoreParams)
where
    T: Into<libp2p::gossipsub::TopicHash>,
{
    params.topics.insert(topic.into(), topic_params);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_decay() {
        // Test decay function produces valid values
        let decay = score_decay(32.0);
        assert!(decay > 0.0 && decay < 1.0);

        // Zero time should return 0
        assert_eq!(score_decay(0.0), 0.0);

        // Longer decay time should produce higher decay factor (slower decay)
        let short_decay = score_decay(10.0);
        let long_decay = score_decay(100.0);
        assert!(short_decay < long_decay);
    }

    #[test]
    fn test_build_peer_score_params() {
        let params =
            build_peer_score_params(Duration::from_secs(12), 32.0, |_params, _epoch_slots| {
                // No topics for this test
            });

        // Verify basic parameters are set correctly
        assert!(params.decay_to_zero > 0.0);
        assert!(params.ip_colocation_factor_weight < 0.0);
        assert!(params.behaviour_penalty_weight < 0.0);
    }
}
