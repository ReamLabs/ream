use std::sync::atomic::{AtomicU64, Ordering};

static ATTESTATION_COMMITTEE_COUNT: AtomicU64 = AtomicU64::new(1);

pub fn attestation_committee_count() -> u64 {
    ATTESTATION_COMMITTEE_COUNT.load(Ordering::Relaxed)
}

/// Set the runtime attestation committee count. Returns the previous value so
/// callers can restore it (used in tests).
pub fn set_attestation_committee_count(value: u64) -> u64 {
    ATTESTATION_COMMITTEE_COUNT.swap(value, Ordering::Relaxed)
}

pub const GOSSIP_DISPARITY_INTERVALS: u64 = 1;
pub const INTERVALS_PER_SLOT: u64 = 5;
pub const MAX_ATTESTATIONS_DATA: u64 = 16;
pub const MAX_HISTORICAL_BLOCK_HASHES: u64 = 262144;
pub const SLOT_DURATION: u64 = 4;
pub const VALIDATOR_REGISTRY_LIMIT: u64 = 4096;

// Validator duty-gate thresholds.
//
// These are client-side policy knobs, not consensus constants. They shape when
// this node signs but do not change what consensus accepts; other clients may
// pick different values without breaking interop.

/// Slot lag past which the local view is treated as too stale to sign duties.
///
/// A vote produced from that view lands on a subtree the network has likely
/// already left, so silencing avoids deposit weight on the wrong branch.
pub const SYNC_LAG_THRESHOLD: u64 = 4;

/// Slot lag past which the whole network is treated as stalled rather than
/// this node lagging.
///
/// When even the freshest locally validated block is this far behind wall
/// clock, the cause is a streak of skipped proposals, not local lag. Duties
/// stay live so the chain can advance through the gap.
pub const NETWORK_STALL_THRESHOLD: u64 = 8;

/// Slot band that keeps the duty gate closed near the threshold.
///
/// Once the gate has closed, it reopens only after the lag drops to
/// `SYNC_LAG_THRESHOLD - HYSTERESIS_BAND`. Prevents single late gossip blocks
/// from flipping the decision slot over slot.
pub const HYSTERESIS_BAND: u64 = 2;
