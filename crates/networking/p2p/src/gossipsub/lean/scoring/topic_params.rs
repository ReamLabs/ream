use std::time::Duration;

use libp2p::gossipsub::TopicScoreParams;

use super::{
    EPOCH_DURATION_SLOTS, SLOT_DURATION_SECS,
    constants::{lean_attestation, lean_block},
};
use crate::gossipsub::{common::scoring::score_decay, lean::topics::LeanGossipTopicKind};

/// Build topic-specific scoring parameters for lean network.
pub fn build_topic_params(kind: &LeanGossipTopicKind) -> TopicScoreParams {
    match kind {
        LeanGossipTopicKind::Block => lean_block_topic_params(),
        LeanGossipTopicKind::Attestation => lean_attestation_topic_params(),
    }
}

/// Lean block topic parameters.
#[allow(clippy::field_reassign_with_default)]
fn lean_block_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = lean_block::TOPIC_WEIGHT;

    params.time_in_mesh_weight = lean_block::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = lean_block::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = lean_block::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay =
        score_decay(lean_block::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.first_message_deliveries_cap = lean_block::FIRST_MESSAGE_DELIVERIES_CAP;

    params.mesh_message_deliveries_weight = lean_block::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay =
        score_decay(lean_block::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.mesh_message_deliveries_cap = lean_block::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold = lean_block::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(lean_block::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(lean_block::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    params.mesh_failure_penalty_weight = lean_block::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay =
        score_decay(lean_block::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params.invalid_message_deliveries_weight = lean_block::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay =
        score_decay(lean_block::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params
}

/// Lean attestation topic parameters.
#[allow(clippy::field_reassign_with_default)]
fn lean_attestation_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = lean_attestation::TOPIC_WEIGHT;

    params.time_in_mesh_weight = lean_attestation::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = lean_attestation::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = lean_attestation::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay =
        score_decay(lean_attestation::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.first_message_deliveries_cap = lean_attestation::FIRST_MESSAGE_DELIVERIES_CAP;

    params.mesh_message_deliveries_weight = lean_attestation::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay =
        score_decay(lean_attestation::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.mesh_message_deliveries_cap = lean_attestation::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold = lean_attestation::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(lean_attestation::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(lean_attestation::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    params.mesh_failure_penalty_weight = lean_attestation::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay =
        score_decay(lean_attestation::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params.invalid_message_deliveries_weight = lean_attestation::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay = score_decay(
        lean_attestation::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );

    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lean_block_params() {
        let params = lean_block_topic_params();

        assert!(params.topic_weight > 0.0);
        assert!(params.first_message_deliveries_weight > 0.0);
        assert!(params.invalid_message_deliveries_weight < 0.0);
        assert!(params.mesh_message_deliveries_weight < 0.0);
    }

    #[test]
    fn test_lean_attestation_params() {
        let params = lean_attestation_topic_params();

        assert!(params.topic_weight > 0.0);
        assert!(params.first_message_deliveries_weight > 0.0);
        assert!(params.invalid_message_deliveries_weight < 0.0);
        assert!(params.mesh_message_deliveries_weight < 0.0);
    }

    #[test]
    fn test_all_lean_topic_kinds_have_params() {
        let kinds = vec![LeanGossipTopicKind::Block, LeanGossipTopicKind::Attestation];

        for kind in kinds {
            let params = build_topic_params(&kind);
            assert!(params.topic_weight > 0.0, "Failed for {kind:?}");
        }
    }
}
