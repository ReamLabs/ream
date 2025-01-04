use super::constants::SLOTS_PER_EPOCH;

pub fn compute_epoch_at_slot(slot: u64) -> u64 {
    slot / SLOTS_PER_EPOCH
}
