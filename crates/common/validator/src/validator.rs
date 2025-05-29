use anyhow::{anyhow, ensure};
use ream_consensus::{
    constants::SLOTS_PER_EPOCH, electra::beacon_state::BeaconState,
    misc::compute_start_slot_at_epoch,
};

pub fn check_if_validator_active(
    state: &BeaconState,
    validator_index: u64,
) -> anyhow::Result<bool> {
    state
        .validators
        .get(validator_index as usize)
        .map(|validator| validator.is_active_validator(state.get_current_epoch()))
        .ok_or_else(|| anyhow!("Validator index out of bounds"))
}

pub fn is_proposer(state: &BeaconState, validator_index: u64) -> anyhow::Result<bool> {
    Ok(state.get_beacon_proposer_index(None)? == validator_index)
}

/// Return the committee assignment in the ``epoch`` for ``validator_index``.
/// ``assignment`` returned is a tuple of the following form:
///     * ``assignment[0]`` is the list of validators in the committee
///     * ``assignment[1]`` is the index to which the committee is assigned
///     * ``assignment[2]`` is the slot at which the committee is assigned
/// Return None if no assignment.
pub fn get_committee_assignment(
    state: &BeaconState,
    epoch: u64,
    validator_index: u64,
) -> anyhow::Result<Option<(Vec<u64>, u64, u64)>> {
    let next_epoch = state.get_current_epoch() + 1;
    ensure!(
        epoch <= next_epoch,
        "Requested epoch {epoch} is beyond the allowed maximum (next epoch: {next_epoch})",
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
