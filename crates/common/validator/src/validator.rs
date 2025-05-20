use anyhow::{Result, ensure};
use ream_consensus::{
    constants::SLOTS_PER_EPOCH, electra::beacon_state::BeaconState,
    misc::compute_start_slot_at_epoch,
};

pub fn check_if_validator_active(state: &BeaconState, validator_index: u64) -> Result<bool> {
    let validator = &state.validators[validator_index as usize];
    Ok(validator.is_active_validator(state.get_current_epoch()))
}

pub fn is_proposer(state: &BeaconState, validator_index: u64) -> Result<bool> {
    Ok(state.get_beacon_proposer_index()? == validator_index)
}

pub fn get_committee_assignment(
    state: &BeaconState,
    epoch: u64,
    validator_index: u64,
) -> Result<Option<(Vec<u64>, u64, u64)>> {
    let next_epoch = state.get_current_epoch() + 1;
    ensure!(
        epoch <= next_epoch,
   "Requested epoch {} is beyond the allowed maximum (next epoch: {})",
    epoch,
    next_epoch
    );

    let start_slot = compute_start_slot_at_epoch(epoch);
    let committee_count_per_slot = state.get_committee_count_per_slot(epoch);

    for slot in start_slot..start_slot + SLOTS_PER_EPOCH {
        for index in 0..committee_count_per_slot {
            let committee = state.get_beacon_committee(slot, index)?;
            if committee.contains(&validator_index) {
                return Ok(Some((committee, index, slot)));
            }
        }
    }
    Ok(None)
}
