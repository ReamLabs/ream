use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    BitList, VariableList,
    typenum::{U262144, U1073741824},
};
use tree_hash_derive::TreeHash;

use crate::{VALIDATOR_REGISTRY_LIMIT, config::Config};

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
    pub fn new(num_validators: u64) -> LeanState {
        LeanState {
            config: Config { num_validators },

            latest_justified_hash: B256::ZERO,
            latest_justified_slot: 0,
            latest_finalized_hash: B256::ZERO,
            latest_finalized_slot: 0,

            historical_block_hashes: VariableList::empty(),
            justified_slots: VariableList::empty(),

            justifications_roots: VariableList::empty(),
            justifications_roots_validators: BitList::with_capacity(0)
                .expect("Failed to initialize state's justifications_roots_validators"),
        }
    }

    fn get_justifications_roots_index(&self, root: &B256) -> Option<usize> {
        self.justifications_roots.iter().position(|r| r == root)
    }

    pub fn initialize_justifications_for_root(&mut self, root: &B256) {
        if !self.justifications_roots.contains(root) {
            self.justifications_roots
                .push(*root)
                .expect("Failed to insert root into justifications_roots");

            let old_length = self.justifications_roots_validators.len();
            let new_length = old_length + VALIDATOR_REGISTRY_LIMIT as usize;

            let mut new_justifications_roots_validators = BitList::with_capacity(new_length)
                .expect("Failed to initialize new justification bits");

            for (i, bit) in self.justifications_roots_validators.iter().enumerate() {
                new_justifications_roots_validators
                    .set(i, bit)
                    .expect("Failed to initialize justification bits to existing values");
            }

            for i in old_length..new_length {
                new_justifications_roots_validators
                    .set(i, false)
                    .expect("Failed to zero-fill justification bits");
            }

            self.justifications_roots_validators = new_justifications_roots_validators;
        }
    }

    pub fn set_justification(&mut self, root: &B256, validator_id: &u64, value: bool) {
        let index = self
            .get_justifications_roots_index(root)
            .expect("Failed to find the justifications index to set");

        self.justifications_roots_validators
            .set(
                index * VALIDATOR_REGISTRY_LIMIT as usize + *validator_id as usize,
                value,
            )
            .expect("Failed to set justification bit");
    }

    pub fn count_justifications(&self, root: &B256) -> u64 {
        let index = self
            .get_justifications_roots_index(root)
            .expect("Could not find justifications for the provided block root");

        let start_range = index * VALIDATOR_REGISTRY_LIMIT as usize;

        self.justifications_roots_validators
            .iter()
            .skip(start_range)
            .take(VALIDATOR_REGISTRY_LIMIT as usize)
            .fold(0, |acc, justification_bits| {
                acc + justification_bits as usize
            }) as u64
    }

    pub fn remove_justifications(&mut self, root: &B256) {
        let index = self
            .get_justifications_roots_index(root)
            .expect("Failed to find the justifications index to remove");
        self.justifications_roots.remove(index);

        let new_length = self.justifications_roots.len() * VALIDATOR_REGISTRY_LIMIT as usize;
        let mut new_justifications_roots_validators =
            BitList::<U1073741824>::with_capacity(new_length)
                .expect("Failed to recreate state's justifications_roots_validators");

        // Take left side of the list (if any)
        self.justifications_roots_validators
            .iter()
            .take(index * VALIDATOR_REGISTRY_LIMIT as usize)
            .fold(0, |i, justification_bit| {
                new_justifications_roots_validators
                    .set(i, justification_bit)
                    .expect("Failed to set new justification bit");
                i + 1
            });

        // Take right side of the list (if any)
        self.justifications_roots_validators
            .iter()
            .skip((index + 1) * VALIDATOR_REGISTRY_LIMIT as usize)
            .fold(
                index * VALIDATOR_REGISTRY_LIMIT as usize,
                |i, justification_bit| {
                    new_justifications_roots_validators
                        .set(i, justification_bit)
                        .expect("Failed to set new justification bit");
                    i + 1
                },
            );

        self.justifications_roots_validators = new_justifications_roots_validators;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn initialize_justifications_for_root() {
        let mut state = LeanState::new(1);

        // Initialize 1st root
        state.initialize_justifications_for_root(&B256::repeat_byte(1));
        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(
            state.justifications_roots_validators.len(),
            VALIDATOR_REGISTRY_LIMIT as usize
        );

        // Initialize an existing root should result in same lengths
        state.initialize_justifications_for_root(&B256::repeat_byte(1));
        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(
            state.justifications_roots_validators.len(),
            VALIDATOR_REGISTRY_LIMIT as usize
        );

        // Initialize 2nd root
        state.initialize_justifications_for_root(&B256::repeat_byte(2));
        assert_eq!(state.justifications_roots.len(), 2);
        assert_eq!(
            state.justifications_roots_validators.len(),
            2 * VALIDATOR_REGISTRY_LIMIT as usize
        );
    }

    #[test]
    fn set_justification() {
        let mut state = LeanState::new(1);
        let root0 = B256::repeat_byte(1);
        let root1 = B256::repeat_byte(2);
        let validator_id = 7u64;

        // Set for 1st root
        state.initialize_justifications_for_root(&root0);
        state.set_justification(&root0, &validator_id, true);
        assert!(
            state
                .justifications_roots_validators
                .get(validator_id as usize)
                .unwrap()
        );

        // Set for 2nd root
        state.initialize_justifications_for_root(&root1);
        state.set_justification(&root1, &validator_id, true);
        assert!(
            state
                .justifications_roots_validators
                .get(VALIDATOR_REGISTRY_LIMIT as usize + validator_id as usize)
                .unwrap()
        );
    }

    #[test]
    fn count_justifications() {
        let mut state = LeanState::new(1);
        let root0 = B256::repeat_byte(1);
        let root1 = B256::repeat_byte(2);

        // Justifications for 1st root, up to 2 justifications
        state.initialize_justifications_for_root(&root0);

        state.set_justification(&root0, &1u64, true);
        assert_eq!(state.count_justifications(&root0), 1);

        state.set_justification(&root0, &2u64, true);
        assert_eq!(state.count_justifications(&root0), 2);

        // Justifications for 2nd root, up to 3 justifications
        state.initialize_justifications_for_root(&root1);

        state.set_justification(&root1, &11u64, true);
        assert_eq!(state.count_justifications(&root1), 1);

        state.set_justification(&root1, &22u64, true);
        state.set_justification(&root1, &33u64, true);
        assert_eq!(state.count_justifications(&root1), 3);
    }

    #[test]
    fn remove_justifications() {
        // Assuming 3 roots & 4 validators
        let mut state = LeanState::new(3);
        let root0 = B256::repeat_byte(1);
        let root1 = B256::repeat_byte(2);
        let root2 = B256::repeat_byte(3);

        // Add justifications for left root
        state.initialize_justifications_for_root(&root0);
        state.set_justification(&root0, &0u64, true);

        // Add justifications for middle root
        state.initialize_justifications_for_root(&root1);
        state.set_justification(&root1, &1u64, true);

        // Add justifications for last root
        state.initialize_justifications_for_root(&root2);
        state.set_justification(&root2, &2u64, true);

        // Assert before removal
        assert_eq!(state.justifications_roots.len(), 3);
        assert_eq!(
            state.justifications_roots_validators.len(),
            3 * VALIDATOR_REGISTRY_LIMIT as usize
        );

        // Assert after removing middle root (root1)
        state.remove_justifications(&root1);

        assert_eq!(
            state.get_justifications_roots_index(&root1),
            None,
            "Root still exists after removal"
        );
        assert_eq!(
            state.justifications_roots.len(),
            2,
            "Should be reduced by 1"
        );
        assert_eq!(
            state.justifications_roots_validators.len(),
            2 * VALIDATOR_REGISTRY_LIMIT as usize,
            "Should be reduced by VALIDATOR_REGISTRY_LIMIT"
        );

        // Assert justifications
        assert!(
            state.justifications_roots_validators.get(0).unwrap(),
            "root0 should still be justified by validator0"
        );
        assert!(
            state
                .justifications_roots_validators
                .get(VALIDATOR_REGISTRY_LIMIT as usize + 2)
                .unwrap(),
            "root2 should still be justified by validator2"
        );
    }
}
