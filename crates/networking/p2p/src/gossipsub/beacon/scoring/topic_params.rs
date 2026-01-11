use std::time::Duration;

use libp2p::gossipsub::TopicScoreParams;

use super::{
    EPOCH_DURATION_SLOTS, SLOT_DURATION_SECS,
    constants::{
        aggregate_and_proof, beacon_attestation, beacon_block, blob_sidecar, light_client,
        slashing, sync_committee, sync_committee_contribution,
    },
};
use crate::gossipsub::{beacon::topics::GossipTopicKind, common::scoring::score_decay};

/// Build topic-specific scoring parameters based on topic type.
///
/// Different topics have different importance and expected message rates,
/// so they require different scoring parameters.
pub fn build_topic_params(kind: &GossipTopicKind) -> TopicScoreParams {
    match kind {
        GossipTopicKind::BeaconBlock => beacon_block_topic_params(),
        GossipTopicKind::AggregateAndProof => aggregate_and_proof_topic_params(),
        GossipTopicKind::BeaconAttestation(_) => beacon_attestation_topic_params(),
        GossipTopicKind::SyncCommittee(_) => sync_committee_topic_params(),
        GossipTopicKind::SyncCommitteeContributionAndProof => {
            sync_committee_contribution_topic_params()
        }
        GossipTopicKind::BlobSidecar(_) => blob_sidecar_topic_params(),
        GossipTopicKind::DataColumnSidecar(_) => data_column_sidecar_topic_params(),
        GossipTopicKind::VoluntaryExit
        | GossipTopicKind::ProposerSlashing
        | GossipTopicKind::AttesterSlashing
        | GossipTopicKind::BlsToExecutionChange => slashing_topic_params(),
        GossipTopicKind::LightClientFinalityUpdate
        | GossipTopicKind::LightClientOptimisticUpdate => light_client_topic_params(),
    }
}

/// Beacon block topic parameters.
///
/// Beacon blocks are the most critical messages in the network.
/// They must be delivered quickly and reliably.
#[allow(clippy::field_reassign_with_default)]
fn beacon_block_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    // Topic weight - beacon blocks are critical
    params.topic_weight = beacon_block::TOPIC_WEIGHT;

    // Time in mesh parameters
    // Peers should stay in mesh for blocks
    params.time_in_mesh_weight = beacon_block::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = beacon_block::TIME_IN_MESH_CAP;

    // First message delivery scoring
    // Reward peers who deliver blocks first
    params.first_message_deliveries_weight = beacon_block::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay =
        score_decay(beacon_block::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.first_message_deliveries_cap = beacon_block::FIRST_MESSAGE_DELIVERIES_CAP;

    // Mesh message delivery parameters
    // Ensure peers are actually delivering messages in the mesh
    params.mesh_message_deliveries_weight = beacon_block::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay =
        score_decay(beacon_block::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.mesh_message_deliveries_cap = beacon_block::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold = beacon_block::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(beacon_block::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(beacon_block::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    // Mesh failure penalty
    params.mesh_failure_penalty_weight = beacon_block::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay =
        score_decay(beacon_block::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    // Invalid message delivery penalty - harsh for blocks
    params.invalid_message_deliveries_weight = beacon_block::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay =
        score_decay(beacon_block::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params
}

/// Aggregate and proof topic parameters.
///
/// Aggregated attestations are important but less critical than blocks.
#[allow(clippy::field_reassign_with_default)]
fn aggregate_and_proof_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = aggregate_and_proof::TOPIC_WEIGHT;

    params.time_in_mesh_weight = aggregate_and_proof::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = aggregate_and_proof::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = aggregate_and_proof::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay = score_decay(
        aggregate_and_proof::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );
    params.first_message_deliveries_cap = aggregate_and_proof::FIRST_MESSAGE_DELIVERIES_CAP;

    // Aggregates should be delivered more frequently than blocks
    params.mesh_message_deliveries_weight = aggregate_and_proof::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay = score_decay(
        aggregate_and_proof::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );
    params.mesh_message_deliveries_cap = aggregate_and_proof::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold =
        aggregate_and_proof::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(aggregate_and_proof::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(aggregate_and_proof::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    params.mesh_failure_penalty_weight = aggregate_and_proof::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay =
        score_decay(aggregate_and_proof::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params.invalid_message_deliveries_weight =
        aggregate_and_proof::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay = score_decay(
        aggregate_and_proof::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );

    params
}

/// Beacon attestation subnet topic parameters.
///
/// Individual attestations on subnets have high volume but lower individual importance.
#[allow(clippy::field_reassign_with_default)]
fn beacon_attestation_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    // Lower weight for individual attestations
    params.topic_weight = beacon_attestation::TOPIC_WEIGHT;

    params.time_in_mesh_weight = beacon_attestation::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = beacon_attestation::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = beacon_attestation::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay = score_decay(
        beacon_attestation::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );
    params.first_message_deliveries_cap = beacon_attestation::FIRST_MESSAGE_DELIVERIES_CAP;

    // Higher delivery threshold for attestation subnets
    params.mesh_message_deliveries_weight = beacon_attestation::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay = score_decay(
        beacon_attestation::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );
    params.mesh_message_deliveries_cap = beacon_attestation::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold =
        beacon_attestation::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(beacon_attestation::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(beacon_attestation::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    params.mesh_failure_penalty_weight = beacon_attestation::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay =
        score_decay(beacon_attestation::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params.invalid_message_deliveries_weight =
        beacon_attestation::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay = score_decay(
        beacon_attestation::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );

    params
}

/// Sync committee topic parameters.
#[allow(clippy::field_reassign_with_default)]
fn sync_committee_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = sync_committee::TOPIC_WEIGHT;

    params.time_in_mesh_weight = sync_committee::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = sync_committee::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = sync_committee::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay =
        score_decay(sync_committee::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.first_message_deliveries_cap = sync_committee::FIRST_MESSAGE_DELIVERIES_CAP;

    params.mesh_message_deliveries_weight = sync_committee::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay =
        score_decay(sync_committee::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.mesh_message_deliveries_cap = sync_committee::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold = sync_committee::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(sync_committee::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(sync_committee::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    params.mesh_failure_penalty_weight = sync_committee::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay =
        score_decay(sync_committee::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params.invalid_message_deliveries_weight = sync_committee::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay =
        score_decay(sync_committee::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params
}

/// Sync committee contribution and proof topic parameters.
#[allow(clippy::field_reassign_with_default)]
fn sync_committee_contribution_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = sync_committee_contribution::TOPIC_WEIGHT;

    params.time_in_mesh_weight = sync_committee_contribution::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = sync_committee_contribution::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight =
        sync_committee_contribution::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay = score_decay(
        sync_committee_contribution::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );
    params.first_message_deliveries_cap = sync_committee_contribution::FIRST_MESSAGE_DELIVERIES_CAP;

    params.mesh_message_deliveries_weight =
        sync_committee_contribution::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay = score_decay(
        sync_committee_contribution::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );
    params.mesh_message_deliveries_cap = sync_committee_contribution::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold =
        sync_committee_contribution::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(sync_committee_contribution::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(sync_committee_contribution::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    params.mesh_failure_penalty_weight = sync_committee_contribution::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay = score_decay(
        sync_committee_contribution::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );

    params.invalid_message_deliveries_weight =
        sync_committee_contribution::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay = score_decay(
        sync_committee_contribution::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS,
    );

    params
}

/// Blob sidecar topic parameters.
///
/// Blob sidecars are important for data availability and should be treated
/// similarly to beacon blocks.
#[allow(clippy::field_reassign_with_default)]
fn blob_sidecar_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = blob_sidecar::TOPIC_WEIGHT;

    params.time_in_mesh_weight = blob_sidecar::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = blob_sidecar::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = blob_sidecar::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay =
        score_decay(blob_sidecar::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.first_message_deliveries_cap = blob_sidecar::FIRST_MESSAGE_DELIVERIES_CAP;

    params.mesh_message_deliveries_weight = blob_sidecar::MESH_MESSAGE_DELIVERIES_WEIGHT;
    params.mesh_message_deliveries_decay =
        score_decay(blob_sidecar::MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.mesh_message_deliveries_cap = blob_sidecar::MESH_MESSAGE_DELIVERIES_CAP;
    params.mesh_message_deliveries_threshold = blob_sidecar::MESH_MESSAGE_DELIVERIES_THRESHOLD;
    params.mesh_message_deliveries_window =
        Duration::from_secs(blob_sidecar::MESH_MESSAGE_DELIVERIES_WINDOW_SECS);
    params.mesh_message_deliveries_activation =
        Duration::from_secs(blob_sidecar::MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS);

    params.mesh_failure_penalty_weight = blob_sidecar::MESH_FAILURE_PENALTY_WEIGHT;
    params.mesh_failure_penalty_decay =
        score_decay(blob_sidecar::MESH_FAILURE_PENALTY_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params.invalid_message_deliveries_weight = blob_sidecar::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay =
        score_decay(blob_sidecar::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params
}

/// Data column sidecar topic parameters.
///
/// Data column sidecars for PeerDAS have similar importance to blob sidecars.
fn data_column_sidecar_topic_params() -> TopicScoreParams {
    // Use the same parameters as blob sidecars for now
    blob_sidecar_topic_params()
}

/// Slashing and exit topic parameters.
///
/// These messages are rare but important. We use relaxed delivery requirements.
#[allow(clippy::field_reassign_with_default)]
fn slashing_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = slashing::TOPIC_WEIGHT;

    params.time_in_mesh_weight = slashing::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = slashing::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = slashing::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay =
        score_decay(slashing::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.first_message_deliveries_cap = slashing::FIRST_MESSAGE_DELIVERIES_CAP;

    // No mesh delivery requirements for rare messages
    params.mesh_message_deliveries_weight = 0.0;
    params.mesh_message_deliveries_decay = 0.0;
    params.mesh_message_deliveries_cap = 0.0;
    params.mesh_message_deliveries_threshold = 0.0;
    params.mesh_message_deliveries_window = Duration::ZERO;
    params.mesh_message_deliveries_activation = Duration::ZERO;

    params.mesh_failure_penalty_weight = 0.0;
    params.mesh_failure_penalty_decay = 0.0;

    // Still penalize invalid messages
    params.invalid_message_deliveries_weight = slashing::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay =
        score_decay(slashing::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params
}

/// Light client topic parameters.
///
/// Light client updates are less frequent and have relaxed requirements.
#[allow(clippy::field_reassign_with_default)]
fn light_client_topic_params() -> TopicScoreParams {
    let mut params = TopicScoreParams::default();

    params.topic_weight = light_client::TOPIC_WEIGHT;

    params.time_in_mesh_weight = light_client::TIME_IN_MESH_WEIGHT;
    params.time_in_mesh_quantum = Duration::from_secs(SLOT_DURATION_SECS);
    params.time_in_mesh_cap = light_client::TIME_IN_MESH_CAP;

    params.first_message_deliveries_weight = light_client::FIRST_MESSAGE_DELIVERIES_WEIGHT;
    params.first_message_deliveries_decay =
        score_decay(light_client::FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);
    params.first_message_deliveries_cap = light_client::FIRST_MESSAGE_DELIVERIES_CAP;

    // No mesh delivery requirements
    params.mesh_message_deliveries_weight = 0.0;
    params.mesh_message_deliveries_decay = 0.0;
    params.mesh_message_deliveries_cap = 0.0;
    params.mesh_message_deliveries_threshold = 0.0;
    params.mesh_message_deliveries_window = Duration::ZERO;
    params.mesh_message_deliveries_activation = Duration::ZERO;

    params.mesh_failure_penalty_weight = 0.0;
    params.mesh_failure_penalty_decay = 0.0;

    params.invalid_message_deliveries_weight = light_client::INVALID_MESSAGE_DELIVERIES_WEIGHT;
    params.invalid_message_deliveries_decay =
        score_decay(light_client::INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS * EPOCH_DURATION_SLOTS);

    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beacon_block_params() {
        let params = beacon_block_topic_params();

        // Topic weight should be positive
        assert!(params.topic_weight > 0.0);

        // First message delivery should be positive (reward)
        assert!(params.first_message_deliveries_weight > 0.0);

        // Invalid message delivery should be negative (penalty)
        assert!(params.invalid_message_deliveries_weight < 0.0);

        // Mesh delivery penalty should be negative
        assert!(params.mesh_message_deliveries_weight < 0.0);
    }

    #[test]
    fn test_all_topic_kinds_have_params() {
        // Ensure all topic kinds produce valid params without panicking
        let kinds = vec![
            GossipTopicKind::BeaconBlock,
            GossipTopicKind::AggregateAndProof,
            GossipTopicKind::BeaconAttestation(0),
            GossipTopicKind::SyncCommittee(0),
            GossipTopicKind::SyncCommitteeContributionAndProof,
            GossipTopicKind::BlobSidecar(0),
            GossipTopicKind::DataColumnSidecar(0),
            GossipTopicKind::VoluntaryExit,
            GossipTopicKind::ProposerSlashing,
            GossipTopicKind::AttesterSlashing,
            GossipTopicKind::BlsToExecutionChange,
            GossipTopicKind::LightClientFinalityUpdate,
            GossipTopicKind::LightClientOptimisticUpdate,
        ];

        for kind in kinds {
            let params = build_topic_params(&kind);
            // All topics should have positive topic weight
            assert!(params.topic_weight > 0.0, "Failed for {kind:?}");
        }
    }
}
