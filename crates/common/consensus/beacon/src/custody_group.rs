use anyhow::{Ok, Result, anyhow};
use discv5::enr::NodeId;
use ream_consensus_misc::constants::beacon::NUM_CUSTODY_GROUPS;
use sha2::{Digest, Sha256};

use crate::data_column_sidecar::NUMBER_OF_COLUMNS;

pub fn get_custody_group_indices(node_id: NodeId, custody_group_count: u64) -> Result<Vec<u64>> {
    if custody_group_count > NUM_CUSTODY_GROUPS {
        return Err(anyhow!(
            "Custody group count more than number of custody groups",
        ));
    }

    if custody_group_count == NUM_CUSTODY_GROUPS {
        return Ok((0..NUM_CUSTODY_GROUPS).collect());
    }

    let mut custody_indices: Vec<u64> = Vec::new();
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
    if custody_group_index >= NUM_CUSTODY_GROUPS {
        return Err(anyhow!(
            "Custody group index is greater than total custody groups"
        ));
    }

    let mut column_indices = Vec::new();
    for col in 0..NUMBER_OF_COLUMNS {
        if col % NUM_CUSTODY_GROUPS == custody_group_index {
            column_indices.push(col);
        }
    }

    Ok(column_indices)
}
