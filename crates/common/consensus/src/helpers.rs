use std::cmp;

use alloy_primitives::B256;

use crate::{
    constants::{EFFECTIVE_BALANCE_INCREMENT, SLOTS_PER_EPOCH},
    deneb::beacon_state::BeaconState,
};

pub fn get_total_balance(state: &BeaconState, indices: Vec<u64>) -> u64 {
    let sum = indices
        .iter()
        .map(|&index| {
            state
                .validators
                .get(index as usize)
                .expect("Couldn't find index invalidators")
                .effective_balance
        })
        .sum();
    cmp::max(EFFECTIVE_BALANCE_INCREMENT, sum)
}

pub fn get_total_active_balance(state: BeaconState) -> u64 {
    get_total_balance(
        &state,
        state.get_active_validator_indices(state.get_current_epoch()),
    )
}

pub fn calculate_committee_fraction(state: BeaconState, committee_percent: u64) -> u64 {
    let committee_weight = get_total_active_balance(state) / SLOTS_PER_EPOCH;
    (committee_weight * committee_percent) / 100
}

pub fn xor<T: AsRef<[u8]>>(bytes_1: T, bytes_2: T) -> B256 {
    let mut result: B256 = B256::default();
    for i in 0..32 {
        result[i] = bytes_1.as_ref()[i] ^ bytes_2.as_ref()[i];
    }
    result
}
