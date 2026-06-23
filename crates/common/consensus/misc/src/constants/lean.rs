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
// The leanspec value is 8. ream's 16 let blocks carry up to 16 distinct
// AttestationData (each ~173KB WHIR proof → ~2.8MB blocks under head-spread),
// doubling block size and slowing gossip propagation vs the spec's 8-cap.
pub const MAX_ATTESTATIONS_DATA: u64 = 8;
pub const MAX_HISTORICAL_BLOCK_HASHES: u64 = 262144;
pub const SLOT_DURATION: u64 = 4;
pub const VALIDATOR_REGISTRY_LIMIT: u64 = 4096;
