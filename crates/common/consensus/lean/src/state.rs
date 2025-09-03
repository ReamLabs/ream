use alloy_primitives::B256;
use anyhow::anyhow;
use ream_consensus_misc::constants::lean::VALIDATOR_REGISTRY_LIMIT;
use ream_metrics::{FINALIZED_SLOT, HEAD_SLOT, JUSTIFIED_SLOT, set_int_gauge_vec};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    BitList, VariableList,
    typenum::{U262144, U1073741824, U4096},
};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::{block::{Block, BlockBody, BlockHeader, SignedBlock}, checkpoint::Checkpoint, config::Config, is_justifiable_slot, vote::Vote};

/// Represents the state of the Lean chain.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#state)
/// for detailed protocol information.
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct LeanState {
    pub config: Config,
    pub slot: u64,
    pub latest_block_header: BlockHeader,

    pub latest_justified: Checkpoint,
    pub latest_finalized: Checkpoint,

    pub historical_block_hashes: VariableList<B256, U262144>,
    pub justified_slots: VariableList<bool, U262144>,

    pub justifications_roots: VariableList<B256, U262144>,
    pub justifications_roots_validators: BitList<U1073741824>,
}

impl LeanState {
    pub fn new(num_validators: u64, genesis_time: u64) -> LeanState {
        LeanState {
            config: Config {
                num_validators,
                genesis_time,
            },
            slot: 0,
            latest_block_header: BlockHeader::default(),

            latest_justified: Checkpoint::default(),
            latest_finalized: Checkpoint::default(),

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

    pub fn state_transition(&mut self, signed_block: &SignedBlock, valid_signatures: bool, validate_result: bool) -> anyhow::Result<()> {
        // Verify signatures
        assert!(valid_signatures, "Signatures are not valid");

        let block = &signed_block.message;

        // Process slots (including those with no blocks) since block
        self.process_slots(block.slot).expect("Failed to process slots");

        // Process block
        self.process_block(&block).expect("Failed to process block");

        // Verify state root
        if validate_result {
            assert!(block.state_root == self.tree_hash_root(), "Block's state root does not match transitioned state root");
        }

        Ok(())
    }

    fn process_slots(&mut self, slot: u64) -> anyhow::Result<()> {
        assert!(self.slot < slot);

        while self.slot < slot {
            self.process_slot()?;
            self.slot += 1;
        }

        Ok(())
    }

    fn process_slot(&mut self) -> anyhow::Result<()> {
        // Cache latest block header state root
        if self.latest_block_header.state_root == B256::ZERO {
            self.latest_block_header.state_root = self.tree_hash_root();
        }

        Ok(())
    }

    pub fn process_block(&mut self, block: &Block) -> anyhow::Result<()> {
        // Send latest head slot to metrics
        set_int_gauge_vec(&HEAD_SLOT, block.slot as i64, &[]);

        self.process_block_header(block)?;
        self.process_operations(&block.body)?;

        Ok(())
    }

    fn process_block_header(&mut self, block: &Block) -> anyhow::Result<()> {
        // // Verify that the slots match
        // assert_eq!(block.slot, self.slot, "Block slot number does not match state slot number");
        // Verify that the block is newer than latest block header
        assert!(block.slot > self.latest_block_header.slot, "Block slot number is not greater than latest block header slot number");
        // Verify that the proposer index is the correct index
        assert_eq!(block.proposer_index, block.slot % self.config.num_validators, "Block proposer index does not match the expected proposer index");
        // // Verify that the parent matches
        // assert_eq!(block.parent_root, self.latest_block_header.tree_hash_root(), "Block parent root does not match latest block header root");

        // Track historical blocks in the state
        self
            .historical_block_hashes
            .push(block.parent_root)
            .map_err(|err| {
                anyhow!("Failed to add block.parent_root to historical_block_hashes: {err:?}")
            })?;

        // genesis block is always justified
        let is_justified = self.latest_block_header.slot == 0;

        self
            .justified_slots
            .push(is_justified)
            .map_err(|err| anyhow!("Failed to add to justified_slots: {err:?}"))?;

        while self.historical_block_hashes.len() < block.slot as usize {
            self
                .justified_slots
                .push(false)
                .map_err(|err| anyhow!("Failed to prefill justified_slots: {err:?}"))?;

            self
                .historical_block_hashes
                // Diverged from Python implementation: uses `B256::ZERO` instead of `None`
                .push(B256::ZERO)
                .map_err(|err| anyhow!("Failed to prefill historical_block_hashes: {err:?}"))?;
        }

        Ok(())
    }

    fn process_operations(&mut self, body: &BlockBody) -> anyhow::Result<()> {
        // Process attestations
        self.process_attestations(&body.votes)?;
        Ok(())
    }

    fn process_attestations(&mut self, votes: &VariableList<Vote, U4096>) -> anyhow::Result<()> {
        for vote in votes {
            // Ignore votes whose source is not already justified,
            // or whose target is not in the history, or whose target is not a
            // valid justifiable slot
            if !self.justified_slots[vote.source.slot as usize]
                || vote.source.root != self.historical_block_hashes[vote.source.slot as usize]
                || vote.target.root != self.historical_block_hashes[vote.target.slot as usize]
                || vote.target.slot <= vote.source.slot
                || !is_justifiable_slot(&self.latest_finalized.slot, &vote.target.slot)
            {
                continue;
            }

            // Track attempts to justify new hashes
            self.initialize_justifications_for_root(&vote.target.root)?;
            self.set_justification(&vote.target.root, &vote.validator_id, true)?;

            let count = self.count_justifications(&vote.target.root)?;

            // If 2/3 voted for the same new valid hash to justify
            if 3 * count >= (2 * self.config.num_validators) {
                self.latest_justified = vote.target.clone();
                self.justified_slots[vote.target.slot as usize] = true;
                set_int_gauge_vec(&JUSTIFIED_SLOT, self.latest_justified.slot as i64, &[]);

                self.remove_justifications(&vote.target.root)?;

                // Finalization: if the target is the next valid justifiable
                // hash after the source
                let is_target_next_valid_justifiable_slot = !((vote.source.slot + 1)..vote.target.slot)
                    .any(|slot| is_justifiable_slot(&self.latest_finalized.slot, &slot));

                if is_target_next_valid_justifiable_slot {
                    self.latest_finalized = vote.source.clone();
                    set_int_gauge_vec(&FINALIZED_SLOT, self.latest_finalized.slot as i64, &[]);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn initialize_justifications_for_root() {
        let mut state = LeanState::new(1, 0);

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
        let mut state = LeanState::new(1, 0);
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
        let mut state = LeanState::new(1, 0);
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
        let mut state = LeanState::new(3, 0);
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
