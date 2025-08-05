use alloy_primitives::B256;
use anyhow::anyhow;
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
                .expect("Failed to initialize an empty BitList"),
        }
    }

    fn get_justifications_roots_index(&self, root: &B256) -> Option<usize> {
        self.justifications_roots.iter().position(|r| r == root)
    }

    pub fn initialize_justifications_for_root(&mut self, root: &B256) -> anyhow::Result<()> {
        if self.justifications_roots.contains(root) {
            return Ok(());
        }

        self.justifications_roots
            .push(*root)
            .map_err(|err| anyhow!("Failed to insert root into justifications_roots: {err:?}"))?;

        let old_length = self.justifications_roots_validators.len();
        let new_length = old_length + VALIDATOR_REGISTRY_LIMIT as usize;

        let mut new_justifications_roots_validators = BitList::with_capacity(new_length)
            .map_err(|err| anyhow!("Failed to initialize new justification bits: {err:?}"))?;

        for (i, bit) in self.justifications_roots_validators.iter().enumerate() {
            new_justifications_roots_validators
                .set(i, bit)
                .map_err(|err| {
                    anyhow!("Failed to initialize justification bits to existing values: {err:?}")
                })?;
        }

        for i in old_length..new_length {
            new_justifications_roots_validators
                .set(i, false)
                .map_err(|err| anyhow!("Failed to zero-fill justification bits: {err:?}"))?;
        }

        self.justifications_roots_validators = new_justifications_roots_validators;

        Ok(())
    }

    pub fn set_justification(
        &mut self,
        root: &B256,
        validator_id: &u64,
        value: bool,
    ) -> anyhow::Result<()> {
        let index = self.get_justifications_roots_index(root).ok_or_else(|| {
            anyhow!("Failed to find the justifications index to set for root: {root}")
        })?;

        self.justifications_roots_validators
            .set(
                index * VALIDATOR_REGISTRY_LIMIT as usize + *validator_id as usize,
                value,
            )
            .map_err(|err| anyhow!("Failed to set justification bit: {err:?}"))?;

        Ok(())
    }

    pub fn count_justifications(&self, root: &B256) -> anyhow::Result<u64> {
        let index = self
            .get_justifications_roots_index(root)
            .ok_or_else(|| anyhow!("Could not find justifications for root: {root}"))?;

        let start_range = index * VALIDATOR_REGISTRY_LIMIT as usize;

        Ok(self
            .justifications_roots_validators
            .iter()
            .skip(start_range)
            .take(VALIDATOR_REGISTRY_LIMIT as usize)
            .fold(0, |acc, justification_bits| {
                acc + justification_bits as usize
            }) as u64)
    }

    pub fn remove_justifications(&mut self, root: &B256) -> anyhow::Result<()> {
        let index = self.get_justifications_roots_index(root).ok_or_else(|| {
            anyhow!("Failed to find the justifications index to remove for root: {root}")
        })?;
        self.justifications_roots.remove(index);

        let new_length = self.justifications_roots.len() * VALIDATOR_REGISTRY_LIMIT as usize;
        let mut new_justifications_roots_validators =
            BitList::<U1073741824>::with_capacity(new_length).map_err(|err| {
                anyhow!("Failed to recreate state's justifications_roots_validators: {err:?}")
            })?;

        // Take left side of the list (if any)
        for (i, justification_bit) in self
            .justifications_roots_validators
            .iter()
            .take(index * VALIDATOR_REGISTRY_LIMIT as usize)
            .enumerate()
        {
            new_justifications_roots_validators
                .set(i, justification_bit)
                .map_err(|err| anyhow!("Failed to set new justification bit: {err:?}"))?;
        }

        // Take right side of the list (if any)
        for (i, justification_bit) in self
            .justifications_roots_validators
            .iter()
            .skip((index + 1) * VALIDATOR_REGISTRY_LIMIT as usize)
            .enumerate()
        {
            new_justifications_roots_validators
                .set(
                    index * VALIDATOR_REGISTRY_LIMIT as usize + i,
                    justification_bit,
                )
                .map_err(|err| anyhow!("Failed to set new justification bit: {err:?}"))?;
        }

        self.justifications_roots_validators = new_justifications_roots_validators;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn initialize_justifications_for_root() {
        let mut state = LeanState::new(1);

        // Initialize 1st root
        state
            .initialize_justifications_for_root(&B256::repeat_byte(1))
            .unwrap();
        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(
            state.justifications_roots_validators.len(),
            VALIDATOR_REGISTRY_LIMIT as usize
        );

        // Initialize an existing root should result in same lengths
        state
            .initialize_justifications_for_root(&B256::repeat_byte(1))
            .unwrap();
        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(
            state.justifications_roots_validators.len(),
            VALIDATOR_REGISTRY_LIMIT as usize
        );

        // Initialize 2nd root
        state
            .initialize_justifications_for_root(&B256::repeat_byte(2))
            .unwrap();
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
        state.initialize_justifications_for_root(&root0).unwrap();
        state
            .set_justification(&root0, &validator_id, true)
            .unwrap();
        assert!(
            state
                .justifications_roots_validators
                .get(validator_id as usize)
                .unwrap()
        );

        // Set for 2nd root
        state.initialize_justifications_for_root(&root1).unwrap();
        state
            .set_justification(&root1, &validator_id, true)
            .unwrap();
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
        state.initialize_justifications_for_root(&root0).unwrap();

        state.set_justification(&root0, &1u64, true).unwrap();
        assert_eq!(state.count_justifications(&root0).unwrap(), 1);

        state.set_justification(&root0, &2u64, true).unwrap();
        assert_eq!(state.count_justifications(&root0).unwrap(), 2);

        // Justifications for 2nd root, up to 3 justifications
        state.initialize_justifications_for_root(&root1).unwrap();

        state.set_justification(&root1, &11u64, true).unwrap();
        assert_eq!(state.count_justifications(&root1).unwrap(), 1);

        state.set_justification(&root1, &22u64, true).unwrap();
        state.set_justification(&root1, &33u64, true).unwrap();
        assert_eq!(state.count_justifications(&root1).unwrap(), 3);
    }

    #[test]
    fn remove_justifications() {
        // Assuming 3 roots & 4 validators
        let mut state = LeanState::new(3);
        let root0 = B256::repeat_byte(1);
        let root1 = B256::repeat_byte(2);
        let root2 = B256::repeat_byte(3);

        // Add justifications for left root
        state.initialize_justifications_for_root(&root0).unwrap();
        state.set_justification(&root0, &0u64, true).unwrap();

        // Add justifications for middle root
        state.initialize_justifications_for_root(&root1).unwrap();
        state.set_justification(&root1, &1u64, true).unwrap();

        // Add justifications for last root
        state.initialize_justifications_for_root(&root2).unwrap();
        state.set_justification(&root2, &2u64, true).unwrap();

        // Assert before removal
        assert_eq!(state.justifications_roots.len(), 3);
        assert_eq!(
            state.justifications_roots_validators.len(),
            3 * VALIDATOR_REGISTRY_LIMIT as usize
        );

        // Assert after removing middle root (root1)
        state.remove_justifications(&root1).unwrap();

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
