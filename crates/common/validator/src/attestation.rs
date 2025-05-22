use ream_consensus::constants::SLOTS_PER_EPOCH;

use crate::constants::ATTESTATION_SUBNET_COUNT;

pub fn compute_subnet_for_attestation(
    committees_per_slot: u64,
    slot: u64,
    committee_index: u64,
) -> u8 {
    let slots_since_epoch_start = slot % SLOTS_PER_EPOCH;
    let committee_since_epoch_start = committees_per_slot * slots_since_epoch_start;
    ((committee_since_epoch_start + committee_index) % ATTESTATION_SUBNET_COUNT) as u8
}
