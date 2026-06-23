use anyhow::{Ok, Result, anyhow, ensure};
use discv5::enr::NodeId;
use ream_consensus_misc::constants::beacon::NUM_CUSTODY_GROUPS;
use ream_network_spec::networks::beacon::beacon_network_spec;
use sha2::{Digest, Sha256};

use crate::{data_column_sidecar::NUMBER_OF_COLUMNS, electra::beacon_state::BeaconState};

pub fn get_validators_custody_requirement(
    state: &BeaconState,
    validator_indices: &[u64],
) -> Result<u64> {
    let mut total_node_balance = 0u128;
    for validator_index in validator_indices {
        let validator = state
            .validators
            .get(*validator_index as usize)
            .ok_or_else(|| anyhow!("Validator index out of bounds: {validator_index}"))?;
        total_node_balance += validator.effective_balance as u128;
    }

    let spec = beacon_network_spec();
    Ok(compute_validators_custody_requirement(
        total_node_balance,
        spec.balance_per_additional_custody_group,
        spec.validator_custody_requirement,
        spec.number_of_custody_groups,
    ))
}

fn compute_validators_custody_requirement(
    total_node_balance: u128,
    balance_per_additional_custody_group: u64,
    validator_custody_requirement: u64,
    number_of_custody_groups: u64,
) -> u64 {
    let count = total_node_balance / balance_per_additional_custody_group as u128;
    let count = count.min(number_of_custody_groups as u128) as u64;
    count
        .max(validator_custody_requirement)
        .min(number_of_custody_groups)
}

pub fn get_custody_group_indices(node_id: NodeId, custody_group_count: u64) -> Result<Vec<u64>> {
    ensure!(
        custody_group_count <= NUM_CUSTODY_GROUPS,
        "Custody group count more than number of custody groups"
    );

    if custody_group_count == NUM_CUSTODY_GROUPS {
        return Ok((0..NUM_CUSTODY_GROUPS).collect());
    }

    let mut custody_indices = Vec::new();
    let mut current_id = node_id.raw();

    while custody_indices.len() < custody_group_count as usize {
        let hash = Sha256::digest(current_id);

        let mut array = [0u8; 8];
        array.copy_from_slice(&hash[0..8]);
        let index = u64::from_le_bytes(array) % NUM_CUSTODY_GROUPS;

        if !custody_indices.contains(&index) {
            custody_indices.push(index);
        }

        let mut carry = true;
        for byte in current_id.iter_mut().rev() {
            if carry {
                let (new_byte, overflow) = byte.overflowing_add(1);
                *byte = new_byte;
                carry = overflow;
            }
        }
    }
    custody_indices.sort();
    Ok(custody_indices)
}

pub fn compute_columns_for_custody_group(custody_group_index: u64) -> Result<Vec<u64>> {
    ensure!(
        custody_group_index < NUM_CUSTODY_GROUPS,
        "Custody group index is greater than total custody groups"
    );

    let mut column_indices = Vec::new();
    for column in 0..NUMBER_OF_COLUMNS {
        if column % NUM_CUSTODY_GROUPS == custody_group_index {
            column_indices.push(column);
        }
    }

    Ok(column_indices)
}

#[cfg(test)]
mod tests {
    use super::compute_validators_custody_requirement;

    const BALANCE_PER_ADDITIONAL_CUSTODY_GROUP: u64 = 32_000_000_000;
    const VALIDATOR_CUSTODY_REQUIREMENT: u64 = 8;
    const NUMBER_OF_CUSTODY_GROUPS: u64 = 128;

    fn compute(total_node_balance: u128) -> u64 {
        compute_validators_custody_requirement(
            total_node_balance,
            BALANCE_PER_ADDITIONAL_CUSTODY_GROUP,
            VALIDATOR_CUSTODY_REQUIREMENT,
            NUMBER_OF_CUSTODY_GROUPS,
        )
    }

    #[test]
    fn custody_requirement_respects_minimum_for_empty_balance() {
        assert_eq!(compute(0), VALIDATOR_CUSTODY_REQUIREMENT);
    }

    #[test]
    fn custody_requirement_respects_minimum_for_low_balance() {
        let total_node_balance = (VALIDATOR_CUSTODY_REQUIREMENT - 1) as u128
            * BALANCE_PER_ADDITIONAL_CUSTODY_GROUP as u128;
        assert_eq!(compute(total_node_balance), VALIDATOR_CUSTODY_REQUIREMENT);
    }

    #[test]
    fn custody_requirement_scales_with_effective_balance() {
        let total_node_balance = 10u128 * BALANCE_PER_ADDITIONAL_CUSTODY_GROUP as u128;
        assert_eq!(compute(total_node_balance), 10);
    }

    #[test]
    fn custody_requirement_caps_at_number_of_custody_groups() {
        let total_node_balance = 10_000u128 * BALANCE_PER_ADDITIONAL_CUSTODY_GROUP as u128;
        assert_eq!(compute(total_node_balance), NUMBER_OF_CUSTODY_GROUPS);
    }
}
