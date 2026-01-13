pub mod constants;
pub mod params;
pub mod topic_params;

use ream_consensus_misc::constants::beacon::SLOTS_PER_EPOCH;

/// Number of slots per epoch (as f64 for scoring calculations).
pub const EPOCH_DURATION_SLOTS: f64 = SLOTS_PER_EPOCH as f64;

/// Slot duration in seconds.
pub const SLOT_DURATION_SECS: u64 = 12;
