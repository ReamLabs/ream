pub mod constants;
pub mod params;
pub mod topic_params;

// Lean network uses justification_lookback_slots, which defaults to 32 similar to beacon
pub const EPOCH_DURATION_SLOTS: f64 = 32.0;

/// Slot duration in seconds for lean network
pub const SLOT_DURATION_SECS: u64 = 12;
