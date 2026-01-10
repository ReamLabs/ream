//! Beacon-specific Peer Scoring Topic Constants
//!
//! These values are derived from the Ethereum consensus specification:
//! https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/p2p-interface.md#peer-scoring
//!
//! All major Ethereum consensus clients (Lighthouse, Prysm, Teku, Nimbus) use these same values
//! to ensure consistent peer scoring behavior across the network.

// ============================================================================
// Beacon Block Topic Parameters
// ============================================================================

pub mod beacon_block {
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
    pub const MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS: u64 = 384; // 32 slots
    pub const MESH_FAILURE_PENALTY_WEIGHT: f64 = -0.717;
    pub const MESH_FAILURE_PENALTY_DECAY_EPOCHS: f64 = 5.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -140.4475;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}

// ============================================================================
// Aggregate and Proof Topic Parameters
// ============================================================================

pub mod aggregate_and_proof {
    pub const TOPIC_WEIGHT: f64 = 0.5;
    pub const TIME_IN_MESH_WEIGHT: f64 = 0.03333;
    pub const TIME_IN_MESH_CAP: f64 = 300.0;
    pub const FIRST_MESSAGE_DELIVERIES_WEIGHT: f64 = 0.5;
    pub const FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 8.0;
    pub const FIRST_MESSAGE_DELIVERIES_CAP: f64 = 23.0;
    pub const MESH_MESSAGE_DELIVERIES_WEIGHT: f64 = -0.5;
    pub const MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 4.0;
    pub const MESH_MESSAGE_DELIVERIES_CAP: f64 = 139.0;
    pub const MESH_MESSAGE_DELIVERIES_THRESHOLD: f64 = 1.0;
    pub const MESH_MESSAGE_DELIVERIES_WINDOW_SECS: u64 = 2;
    pub const MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS: u64 = 192; // 16 slots
    pub const MESH_FAILURE_PENALTY_WEIGHT: f64 = -0.5;
    pub const MESH_FAILURE_PENALTY_DECAY_EPOCHS: f64 = 4.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -140.4475;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}

// ============================================================================
// Beacon Attestation Topic Parameters
// ============================================================================

pub mod beacon_attestation {
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
    pub const MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS: u64 = 204; // 17 slots
    pub const MESH_FAILURE_PENALTY_WEIGHT: f64 = -0.336;
    pub const MESH_FAILURE_PENALTY_DECAY_EPOCHS: f64 = 4.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -34.55;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}

// ============================================================================
// Sync Committee Topic Parameters
// ============================================================================

pub mod sync_committee {
    pub const TOPIC_WEIGHT: f64 = 0.4;
    pub const TIME_IN_MESH_WEIGHT: f64 = 0.03333;
    pub const TIME_IN_MESH_CAP: f64 = 300.0;
    pub const FIRST_MESSAGE_DELIVERIES_WEIGHT: f64 = 0.5;
    pub const FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 8.0;
    pub const FIRST_MESSAGE_DELIVERIES_CAP: f64 = 23.0;
    pub const MESH_MESSAGE_DELIVERIES_WEIGHT: f64 = -0.5;
    pub const MESH_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 4.0;
    pub const MESH_MESSAGE_DELIVERIES_CAP: f64 = 139.0;
    pub const MESH_MESSAGE_DELIVERIES_THRESHOLD: f64 = 2.0;
    pub const MESH_MESSAGE_DELIVERIES_WINDOW_SECS: u64 = 2;
    pub const MESH_MESSAGE_DELIVERIES_ACTIVATION_SECS: u64 = 192;
    pub const MESH_FAILURE_PENALTY_WEIGHT: f64 = -0.5;
    pub const MESH_FAILURE_PENALTY_DECAY_EPOCHS: f64 = 4.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -70.22;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}

// ============================================================================
// Sync Committee Contribution Topic Parameters (same as sync_committee)
// ============================================================================

pub mod sync_committee_contribution {
    pub use super::sync_committee::*;
}

// ============================================================================
// Blob Sidecar Topic Parameters (same as beacon blocks)
// ============================================================================

pub mod blob_sidecar {
    pub use super::beacon_block::*;
}

// ============================================================================
// Slashing/Exit Topic Parameters
// ============================================================================

pub mod slashing {
    pub const TOPIC_WEIGHT: f64 = 0.05;
    pub const TIME_IN_MESH_WEIGHT: f64 = 0.03333;
    pub const TIME_IN_MESH_CAP: f64 = 300.0;
    pub const FIRST_MESSAGE_DELIVERIES_WEIGHT: f64 = 0.1;
    pub const FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 100.0;
    pub const FIRST_MESSAGE_DELIVERIES_CAP: f64 = 10.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -2000.0;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}

// ============================================================================
// Light Client Topic Parameters
// ============================================================================

pub mod light_client {
    pub const TOPIC_WEIGHT: f64 = 0.05;
    pub const TIME_IN_MESH_WEIGHT: f64 = 0.03333;
    pub const TIME_IN_MESH_CAP: f64 = 300.0;
    pub const FIRST_MESSAGE_DELIVERIES_WEIGHT: f64 = 0.1;
    pub const FIRST_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 20.0;
    pub const FIRST_MESSAGE_DELIVERIES_CAP: f64 = 10.0;
    pub const INVALID_MESSAGE_DELIVERIES_WEIGHT: f64 = -2000.0;
    pub const INVALID_MESSAGE_DELIVERIES_DECAY_EPOCHS: f64 = 50.0;
}
