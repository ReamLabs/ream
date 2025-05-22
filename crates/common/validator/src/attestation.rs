use ream_consensus::constants::SLOTS_PER_EPOCH;

use crate::constants::ATTESTATION_SUBNET_COUNT;

/// Compute the correct subnet for an attestation for Phase 0.
/// Note, this mimics expected future behavior where attestations will be mapped to their shard
/// subnet.
pub fn compute_subnet_for_attestation(
    committees_per_slot: u64,
    slot: u64,
    committee_index: u64,
) -> u64 {
    let slots_since_epoch_start = slot % SLOTS_PER_EPOCH;
    let committee_since_epoch_start = committees_per_slot * slots_since_epoch_start;
    (committee_since_epoch_start + committee_index) % ATTESTATION_SUBNET_COUNT
}
