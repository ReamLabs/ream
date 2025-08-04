use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    BitList, VariableList,
    typenum::{U262144, U1073741824, Unsigned},
};
use tree_hash_derive::TreeHash;

use crate::{
    config::Config,
    MAX_HISTORICAL_BLOCK_HASHES,
    VALIDATOR_REGISTRY_LIMIT,
};

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct LeanState {
    pub config: Config,

    pub latest_justified_hash: B256,
    pub latest_justified_slot: u64,
    pub latest_finalized_hash: B256,
    pub latest_finalized_slot: u64,

    pub historical_block_hashes: VariableList<B256, U262144>,
    pub justified_slots: VariableList<bool, U262144>,

    // Diverged from Python implementation:
    // Originally `justifications: Dict[str, List[bool]]`
    pub justifications_roots: VariableList<B256, U262144>,
    // The size is MAX_HISTORICAL_BLOCK_HASHES * VALIDATOR_REGISTRY_LIMIT
    // to accommodate equivalent to `justifications[root][validator_id]`
    pub justifications_roots_validators: BitList<U1073741824>,
}

impl LeanState {
    fn get_justifications_roots_index(&self, root: &B256) -> Option<usize> {
        self.justifications_roots.iter().position(|r| r == root)
    }

    fn get_justifications_roots_range(&self, index: &usize) -> (usize, usize) {
        let start_range = index * MAX_HISTORICAL_BLOCK_HASHES as usize;
        let end_range = start_range + VALIDATOR_REGISTRY_LIMIT as usize;

        (start_range, end_range)
    }

    pub fn initialize_justifications_for_root(&mut self, root: &B256) {
        if !self.justifications_roots.contains(root) {
            self.justifications_roots
                .push(*root)
                .expect("Failed to insert root into justifications_roots");
        }
    }

    pub fn set_justification(&mut self, root: &B256, validator_id: &u64, value: bool) {
        let index = self
            .get_justifications_roots_index(root)
            .expect("Failed to find the justifications index to set");
        self.justifications_roots_validators
            .set(index * U262144::to_usize() + *validator_id as usize, value)
            .expect("Failed to set justification bit");
    }

    pub fn count_justifications(&self, root: &B256) -> u64 {
        let index = self
            .get_justifications_roots_index(root)
            .expect("Could not find justifications for the provided block root");

        let (start_range, end_range) = self.get_justifications_roots_range(&index);

        self.justifications_roots_validators.as_slice()[start_range..end_range]
            .iter()
            .fold(0, |acc, justification_bits| {
                acc + justification_bits.count_ones()
            }) as u64
    }

    pub fn remove_justifications(&mut self, root: &B256) {
        // Remove from `state.justifications_roots`
        let index = self
            .get_justifications_roots_index(root)
            .expect("Failed to find the justifications index to remove");
        self.justifications_roots.remove(index);

        let (start_range, end_range) = self.get_justifications_roots_range(&index);

        // Remove from `state.justifications_roots_validators`
        for i in start_range..end_range {
            self.justifications_roots_validators
                .set(i, false)
                .expect("Failed to remove justifications");
        }
    }
}
