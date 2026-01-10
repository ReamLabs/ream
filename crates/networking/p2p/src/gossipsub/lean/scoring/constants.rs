//! Lean Network Peer Scoring Topic Constants
//!
//! Lean network has a simpler topic structure with only blocks and attestations.
//! These parameters are adapted from Ethereum consensus specs but adjusted for
//! the lean network's different message rates and validation requirements.

// ============================================================================
// Lean Block Topic Parameters
// ============================================================================

pub mod lean_block {
    // Similar to beacon blocks, lean blocks are critical
    pub const TOPIC_WEIGHT: f64 = 0.5;
    pub const TIME_IN_MESH_WEIGHT: f64 = 0.03333;
    pub const TIME_IN_MESH_CAP: f64 = 300.0;
    pub const FIRST_MESSAGE_DELIVERIES_WEIGHT: f64 = 1.0;
    pub const FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 20.0;
    pub const FIRST_MESSAGE_DELIVERIES_CAP: f64 = 23.0;
    pub const MESH_MESSAGE_DELIVERIES_WEIGHT: f64 = -0.717;
    pub const MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 5.0;
    pub const MESH_MESSAGE_DELIVERIES_CAP: f64 = 139.0;
    pub const MESH_MESSAGE_DELIVERIES_THRESHOLD: f64 = 1.0;
    pub const MESH_MESSAGE_DELIVERIES_WINDOW_SECS: u64 = 2;
    pub const MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS: u64 = 384;
    pub const MESH_FAILURE_PENALTY_WEIGHT: f64 = -0.717;
    pub const MESH_FAILURE_PENALTY_DECAY_EPOCHS: f64 = 5.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -140.4475;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}

// ============================================================================
// Lean Attestation Topic Parameters
// ============================================================================

pub mod lean_attestation {
    // Similar to beacon attestations
    pub const TOPIC_WEIGHT: f64 = 0.25;
    pub const TIME_IN_MESH_WEIGHT: f64 = 0.03333;
    pub const TIME_IN_MESH_CAP: f64 = 300.0;
    pub const FIRST_MESSAGE_DELIVERIES_WEIGHT: f64 = 0.336;
    pub const FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 8.0;
    pub const FIRST_MESSAGE_DELIVERIES_CAP: f64 = 23.0;
    pub const MESH_MESSAGE_DELIVERIES_WEIGHT: f64 = -0.336;
    pub const MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 4.0;
    pub const MESH_MESSAGE_DELIVERIES_CAP: f64 = 139.0;
    pub const MESH_MESSAGE_DELIVERIES_THRESHOLD: f64 = 4.0;
    pub const MESH_MESSAGE_DELIVERIES_WINDOW_SECS: u64 = 2;
    pub const MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS: u64 = 204;
    pub const MESH_FAILURE_PENALTY_WEIGHT: f64 = -0.336;
    pub const MESH_FAILURE_PENALTY_DECAY_EPOCHS: f64 = 4.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -34.55;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}
