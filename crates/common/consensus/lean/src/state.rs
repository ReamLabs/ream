use alloy_primitives::B256;
use ethereum_hashing::hash;
use serde::{Deserialize, Serialize};
use ssz_types::{
    BitList, VariableList,
    typenum::{U4096, U16777216, Unsigned},
};

use crate::config::Config;

// TODO: Add back #[derive(Encode, Decode, TreeHash)]
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct LeanState {
    pub config: Config,

    pub latest_justified_hash: B256,
    pub latest_justified_slot: u64,
    pub latest_finalized_hash: B256,
    pub latest_finalized_slot: u64,

    pub historical_block_hashes: VariableList<Option<B256>, U4096>,
    pub justified_slots: VariableList<bool, U4096>,

    // Originally `justifications: Dict[str, List[bool]]`
    pub justifications_roots: VariableList<B256, U4096>,
    pub justifications_roots_validators: BitList<U16777216>,
}

impl LeanState {
    pub fn compute_hash(&self) -> B256 {
        let serialized = serde_json::to_string(self).unwrap();
        B256::from_slice(&hash(serialized.as_bytes()))
    }

    fn get_justifications_roots_index(&self, root: &B256) -> Option<usize> {
        self.justifications_roots.iter().position(|r| r == root)
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
            .set(index * U4096::to_usize() + *validator_id as usize, value)
            .expect("Failed to set justification bit");
    }

    pub fn count_justifications(&self, root: &B256) -> u64 {
        let index = self
            .get_justifications_roots_index(root)
            .expect("Could not find justifications for the provided block root");

        let start_range = index * U4096::to_usize();
        let end_range = start_range + U4096::to_usize();

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

        let start_range = index * U4096::to_usize();
        let end_range = start_range + U4096::to_usize();

        // Remove from `state.justifications_roots_validators`
        for i in start_range..end_range {
            self.justifications_roots_validators
                .set(i, false)
                .expect("Failed to remove justifications");
        }
    }
}
