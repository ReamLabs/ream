use std::collections::HashMap;

use alloy_primitives::B256;
use anyhow::{Context, anyhow, ensure};
use itertools::Itertools;
use ream_metrics::{FINALIZED_SLOT, JUSTIFIED_SLOT, set_int_gauge_vec};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    BitList, VariableList,
    typenum::{U4096, U262144, U1073741824},
};
use tracing::info;
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::{
    block::{Block, BlockBody, BlockHeader, SignedBlock},
    checkpoint::Checkpoint,
    config::Config,
    is_justifiable_slot,
    validator::Validator,
    vote::SignedVote,
};

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
    pub justified_slots: BitList<U262144>,

    pub validators: VariableList<Validator, U262144>,

    pub justifications_roots: VariableList<B256, U262144>,
    pub justifications_validators: BitList<U1073741824>,
}

impl LeanState {
    pub fn new(num_validators: u64, genesis_time: u64) -> LeanState {
        LeanState {
            config: Config {
                num_validators,
                genesis_time,
            },
            slot: 0,
            latest_block_header: BlockHeader {
                body_root: BlockBody::default().tree_hash_root(),
                ..BlockHeader::default()
            },

            latest_justified: Checkpoint::default(),
            latest_finalized: Checkpoint::default(),

            historical_block_hashes: VariableList::empty(),
            justified_slots: BitList::with_capacity(0)
                .expect("Failed to initialize an empty BitList"),

            validators: VariableList::empty(),

            justifications_roots: VariableList::empty(),
            justifications_validators: BitList::with_capacity(0)
                .expect("Failed to initialize an empty BitList"),
        }
    }

    /// Returns a map of `root -> justifications` constructed from the
    /// flattened data in the state.
    pub fn get_justifications(&self) -> anyhow::Result<HashMap<B256, BitList<U4096>>> {
        let mut justifications = HashMap::new();

        // Loop each root and reconstruct the justifications from the flattened BitList
        for (i, root) in self.justifications_roots.iter().enumerate() {
            let mut votes_list = BitList::with_capacity(self.config.num_validators as usize)
                .map_err(|err| anyhow!("Failed to create BitList for justifications: {err:?}"))?;

            // Loop each validator and set their justification
            self.justifications_validators
                .iter()
                .skip(i * self.config.num_validators as usize)
                .take(self.config.num_validators as usize)
                .enumerate()
                .try_for_each(|(validator_index, justification)| -> anyhow::Result<()> {
                    votes_list
                        .set(validator_index, justification)
                        .map_err(|err| anyhow!("Failed to set justification: {err:?}"))?;
                    Ok(())
                })?;

            // Insert the root and its justifications into the map
            justifications.insert(*root, votes_list);
        }

        Ok(justifications)
    }

    /// Saves a map of `root -> justifications` back into the state's flattened
    /// data structure.
    pub fn set_justifications(
        &mut self,
        justifications: HashMap<B256, BitList<U4096>>,
    ) -> anyhow::Result<()> {
        let mut justifications_roots = VariableList::<B256, U262144>::empty();
        let mut flattened_justifications = Vec::new();

        for root in justifications.keys().sorted() {
            let justifications_for_root = justifications
                .get(root)
                .ok_or_else(|| anyhow!("Root {root} not found in justifications"))?;

            // Assert that votes list has exactly num_validators items.
            // If the length is incorrect, the constructed bitlist will be corrupt.
            ensure!(
                justifications_for_root.len() == self.config.num_validators as usize,
                "Justifications length ({}) does not match validators length ({})",
                justifications_for_root.len(),
                self.config.num_validators
            );

            justifications_roots
                .push(*root)
                .map_err(|err| anyhow!("Failed to add root to justifications_roots: {err:?}"))?;

            justifications_for_root
                .iter()
                .for_each(|justification| flattened_justifications.push(justification));
        }

        // Create a new Bitlist with all the flattened votes
        let mut justifications_validators =
            BitList::with_capacity(justifications.len() * self.config.num_validators as usize)
                .map_err(|err| {
                    anyhow!("Failed to create BitList for justifications_validators: {err:?}")
                })?;

        flattened_justifications.iter().enumerate().try_for_each(
            |(index, justification)| -> anyhow::Result<()> {
                justifications_validators
                    .set(index, *justification)
                    .map_err(|err| anyhow!("Failed to set justification bit: {err:?}"))
            },
        )?;

        self.justifications_roots = justifications_roots;
        self.justifications_validators = justifications_validators;

        Ok(())
    }

    pub fn state_transition(
        &mut self,
        signed_block: &SignedBlock,
        valid_signatures: bool,
        validate_result: bool,
    ) -> anyhow::Result<()> {
        // Verify signatures
        ensure!(valid_signatures, "Signatures are not valid");

        let block = &signed_block.message;

        // Process slots (including those with no blocks) since block
        self.process_slots(block.slot)
            .context("Failed to process slots")?;

        // Process block
        self.process_block(block)
            .context("Failed to process block")?;

        // Verify state root
        if validate_result {
            ensure!(
                block.state_root == self.tree_hash_root(),
                "Block's state root does not match transitioned state root"
            );
        }

        Ok(())
    }

    fn process_slots(&mut self, slot: u64) -> anyhow::Result<()> {
        ensure!(self.slot < slot, "State slot must be less than block slot");

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

    fn process_block(&mut self, block: &Block) -> anyhow::Result<()> {
        self.process_block_header(block)?;
        self.process_operations(&block.body)?;

        Ok(())
    }

    fn process_block_header(&mut self, block: &Block) -> anyhow::Result<()> {
        // Verify that the slots match
        ensure!(
            block.slot == self.slot,
            "Block slot number does not match state slot number"
        );
        // Verify that the block is newer than latest block header
        ensure!(
            block.slot > self.latest_block_header.slot,
            "Block slot number is not greater than latest block header slot number"
        );
        // Verify that the proposer index is the correct index
        ensure!(
            block.proposer_index == block.slot % self.config.num_validators,
            "Block proposer index does not match the expected proposer index"
        );

        // Verify that the parent matches
        ensure!(
            block.parent_root == self.latest_block_header.tree_hash_root(),
            "Block parent root does not match latest block header root"
        );

        // If this was first block post genesis, 3sf mini special treatment is required
        // to correctly set genesis block root as already justified and finalized.
        // This is not possible at the time of genesis state generation and are set at
        // zero bytes because genesis block is calculated using genesis state causing a
        // circular dependancy
        if self.latest_block_header.slot == 0 {
            // block.parent_root is the genesis root
            self.latest_justified.root = block.parent_root;
            self.latest_finalized.root = block.parent_root;
        }

        // now that we can vote on parent, push it at its correct slot index in the structures
        self.historical_block_hashes
            .push(block.parent_root)
            .map_err(|err| {
                anyhow!("Failed to add block.parent_root to historical_block_hashes: {err:?}")
            })?;

        // genesis block is always justified
        let length = self.justified_slots.len();
        let mut new_bitlist = BitList::with_capacity(length + 1)
            .map_err(|err| anyhow!("Failed to resize justified_slots BitList: {err:?}"))?;
        new_bitlist
            .set(length, self.latest_block_header.slot == 0)
            .map_err(|err| {
                anyhow!(
                    "Failed to set justified slot for slot {}: {err:?}",
                    self.latest_block_header.slot
                )
            })?;
        self.justified_slots = new_bitlist.union(&self.justified_slots);

        // if there were empty slots, push zero hash for those ancestors
        let num_empty_slots = block.slot - self.latest_block_header.slot - 1;
        for _ in 0..num_empty_slots {
            self.historical_block_hashes
                .push(B256::ZERO)
                .map_err(|err| anyhow!("Failed to prefill historical_block_hashes: {err:?}"))?;

            let length = self.justified_slots.len();
            let mut new_bitlist = BitList::with_capacity(length + 1)
                .map_err(|err| anyhow!("Failed to resize justified_slots BitList: {err:?}"))?;
            new_bitlist.set(length, false).map_err(|err| {
                anyhow!(
                    "Failed to set justified slot for empty slot {}: {err:?}",
                    length
                )
            })?;
            self.justified_slots = new_bitlist.union(&self.justified_slots);
        }

        // Cache current block as the new latest block
        self.latest_block_header = BlockHeader {
            slot: block.slot,
            proposer_index: block.proposer_index,
            parent_root: block.parent_root,
            // Overwritten in the next process_slot call
            state_root: B256::ZERO,
            body_root: block.body.tree_hash_root(),
        };

        Ok(())
    }

    fn process_operations(&mut self, body: &BlockBody) -> anyhow::Result<()> {
        // Process attestations
        self.process_attestations(&body.attestations)?;
        // other operations will get added as the functionality evolves
        Ok(())
    }

    pub fn process_attestations(
        &mut self,
        attestations: &VariableList<SignedVote, U4096>,
    ) -> anyhow::Result<()> {
        // get justifications, justified slots and historical block hashes are
        // already up to date as per the processing in process_block_header
        let mut justifications_map = self.get_justifications()?;

        for signed_vote in attestations {
            let vote = &signed_vote.message;
            // Ignore votes whose source is not already justified,
            // or whose target is not in the history, or whose target is not a
            // valid justifiable slot
            if !self
                .justified_slots
                .get(vote.source.slot as usize)
                .map_err(|err| anyhow!("Failed to get justified slot: {err:?}"))?
            {
                info!(
                    reason = "Source slot not justified",
                    source_slot = vote.source.slot,
                    target_slot = vote.target.slot,
                    "Skipping vote by Validator {}",
                    signed_vote.validator_id,
                );
                continue;
            }

            // This condition is missing in 3sf mini but has been added here because
            // we don't want to re-introduce the target again for remaining votes if
            // the slot is already justified and its tracking already cleared out
            // from justifications map
            if self
                .justified_slots
                .get(vote.target.slot as usize)
                .map_err(|err| anyhow!("Failed to get justified slot: {err:?}"))?
            {
                info!(
                    reason = "Target slot already justified",
                    source_slot = vote.source.slot,
                    target_slot = vote.target.slot,
                    "Skipping vote by Validator {}",
                    signed_vote.validator_id,
                );
                continue;
            }

            if vote.source.root
                != *self
                    .historical_block_hashes
                    .get(vote.source.slot as usize)
                    .ok_or(anyhow!("Source slot not found in historical_block_hashes"))?
            {
                info!(
                    reason = "Source block not in historical block hashes",
                    source_slot = vote.source.slot,
                    target_slot = vote.target.slot,
                    "Skipping vote by Validator {}",
                    signed_vote.validator_id,
                );
                continue;
            }

            if vote.target.root
                != *self
                    .historical_block_hashes
                    .get(vote.target.slot as usize)
                    .ok_or(anyhow!("Target slot not found in historical_block_hashes"))?
            {
                info!(
                    reason = "Target block not in historical block hashes",
                    source_slot = vote.source.slot,
                    target_slot = vote.target.slot,
                    "Skipping vote by Validator {}",
                    signed_vote.validator_id,
                );
                continue;
            }

            if vote.target.slot <= vote.source.slot {
                info!(
                    reason = "Target slot not greater than source slot",
                    source_slot = vote.source.slot,
                    target_slot = vote.target.slot,
                    "Skipping vote by Validator {}",
                    signed_vote.validator_id,
                );
                continue;
            }

            if !is_justifiable_slot(self.latest_finalized.slot, vote.target.slot) {
                info!(
                    reason = "Target slot not justifiable",
                    source_slot = vote.source.slot,
                    target_slot = vote.target.slot,
                    "Skipping vote by Validator {}",
                    signed_vote.validator_id,
                );
                continue;
            }

            // Track attempts to justify new hashes
            let justifications = justifications_map.entry(vote.target.root).or_insert(
                BitList::with_capacity(self.config.num_validators as usize).map_err(|err| {
                    anyhow!(
                        "Failed to initialize justification for root {:?}: {err:?}",
                        &vote.target.root
                    )
                })?,
            );

            justifications
                .set(signed_vote.validator_id as usize, true)
                .map_err(|err| {
                    anyhow!(
                        "Failed to set validator {:?}'s justification for root {:?}: {err:?}",
                        signed_vote.validator_id,
                        &vote.target.root
                    )
                })?;

            let count = justifications.num_set_bits();

            // If 2/3 voted for the same new valid hash to justify
            // in 3sf mini this is strict equality, but we have updated it to >=
            // also have modified it from count >= (2 * state.config.num_validators) // 3
            // to prevent integer division which could lead to less than 2/3 of validators
            // justifying specially if the num_validators is low in testing scenarios
            if 3 * count >= (2 * self.config.num_validators) as usize {
                self.latest_justified = vote.target.clone();
                self.justified_slots
                    .set(vote.target.slot as usize, true)
                    .map_err(|err| {
                        anyhow!(
                            "Failed to set justified slot for slot {}: {err:?}",
                            vote.target.slot
                        )
                    })?;

                justifications_map.remove(&vote.target.root);

                info!(
                    slot = self.latest_justified.slot,
                    root = ?self.latest_justified.root,
                    "Justification event",
                );
                set_int_gauge_vec(&JUSTIFIED_SLOT, self.latest_justified.slot as i64, &[]);

                // Finalization: if the target is the next valid justifiable
                // hash after the source
                let is_target_next_valid_justifiable_slot = !((vote.source.slot + 1)
                    ..vote.target.slot)
                    .any(|slot| is_justifiable_slot(self.latest_finalized.slot, slot));

                if is_target_next_valid_justifiable_slot {
                    self.latest_finalized = vote.source.clone();

                    info!(
                        slot = self.latest_finalized.slot,
                        root = ?self.latest_finalized.root,
                        "Finalization event",
                    );
                    set_int_gauge_vec(&FINALIZED_SLOT, self.latest_finalized.slot as i64, &[]);
                }
            }
        }

        // flatten and set updated justifications back to the state
        self.set_justifications(justifications_map)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn get_justifications_empty() {
        let state = LeanState::new(10, 0);

        // Ensure the base state has empty justification lists
        assert_eq!(state.justifications_roots.len(), 0);
        assert_eq!(state.justifications_validators.num_set_bits(), 0);

        let justifications_map = state.get_justifications().unwrap();
        assert_eq!(justifications_map, HashMap::new());
    }

    #[test]
    fn get_justifications_single_root() {
        let num_validators: u64 = 3;
        let mut state = LeanState::new(num_validators, 0);
        state.justifications_validators = BitList::with_capacity(num_validators as usize).unwrap();
        let root = B256::repeat_byte(1);

        state.justifications_roots.push(root).unwrap();
        state.justifications_validators.set(1, true).unwrap();

        let mut expected_bitlist =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();
        expected_bitlist.set(1, true).unwrap();

        let mut expected_map = HashMap::<B256, BitList<U4096>>::new();
        expected_map.insert(root, expected_bitlist);

        let justifications_map = state.get_justifications().unwrap();
        assert_eq!(justifications_map, expected_map);
    }

    #[test]
    fn get_justifications_multiple_roots() {
        let num_validators = 3;
        let mut state = LeanState::new(num_validators as u64, 0);
        state.justifications_validators = BitList::with_capacity(num_validators * 3).unwrap();
        let root0 = B256::repeat_byte(0);
        let root1 = B256::repeat_byte(1);
        let root2 = B256::repeat_byte(2);

        // root0 is voted by validator 0
        state.justifications_roots.push(root0).unwrap();
        state.justifications_validators.set(0, true).unwrap();

        // root1 is voted by validator 1 and 2
        state.justifications_roots.push(root1).unwrap();
        state
            .justifications_validators
            .set(state.config.num_validators as usize + 1, true)
            .unwrap();
        state
            .justifications_validators
            .set(state.config.num_validators as usize + 2, true)
            .unwrap();

        // root2 is voted by none
        state.justifications_roots.push(root2).unwrap();

        // Verify that the reconstructed map is identical to the expected map
        // Because HashMap is not ordered, we need to check root by root
        let justifications = state.get_justifications().unwrap();

        // check root0 voted by validator 0
        let mut expected_bitlist0 =
            BitList::with_capacity(state.config.num_validators as usize).unwrap();
        expected_bitlist0.set(0, true).unwrap();
        assert_eq!(justifications[&root0], expected_bitlist0);

        // Prepare expected root1 voted by validator 1 and 2
        let mut expected_bitlist1 =
            BitList::with_capacity(state.config.num_validators as usize).unwrap();
        expected_bitlist1.set(1, true).unwrap();
        expected_bitlist1.set(2, true).unwrap();
        assert_eq!(justifications[&root1], expected_bitlist1);

        // Prepare expected root2 voted by none
        let expected_bitlist2 =
            BitList::with_capacity(state.config.num_validators as usize).unwrap();
        assert_eq!(justifications[&root2], expected_bitlist2);

        // Also verify that the number of roots matches
        assert_eq!(justifications.len(), 3);
    }

    #[test]
    fn set_justifications_empty() {
        let mut state = LeanState::new(10, 0);
        state.justifications_validators =
            BitList::with_capacity(state.config.num_validators as usize).unwrap();
        state
            .justifications_roots
            .push(B256::repeat_byte(1))
            .unwrap();
        state.justifications_validators.set(0, true).unwrap();

        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(state.justifications_validators.num_set_bits(), 1);

        let justifications = HashMap::<B256, BitList<U4096>>::new();
        state.set_justifications(justifications).unwrap();

        assert_eq!(state.justifications_roots.len(), 0);
        assert_eq!(state.justifications_validators.num_set_bits(), 0);
    }

    #[test]
    fn set_justifications_deterministic_order() {
        let mut state = LeanState::new(10, 0);
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // root0 voted by validator0
        let root0 = B256::repeat_byte(0);
        let mut bitlist0 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();
        bitlist0.set(0, true).unwrap();

        // root1 voted by validator1
        let root1 = B256::repeat_byte(1);
        let mut bitlist1 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();
        bitlist1.set(1, true).unwrap();

        // root2 voted by validator2
        let root2 = B256::repeat_byte(2);
        let mut bitlist2 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();
        bitlist2.set(2, true).unwrap();

        // Insert unordered: root0, root2, root1
        justifications.insert(root0, bitlist0);
        justifications.insert(root2, bitlist2);
        justifications.insert(root1, bitlist1);

        state.set_justifications(justifications).unwrap();

        assert_eq!(state.justifications_roots[0], B256::repeat_byte(0));
        assert!(state.justifications_validators.get(0).unwrap());
        assert_eq!(state.justifications_roots[1], B256::repeat_byte(1));
        assert!(
            state
                .justifications_validators
                .get(state.config.num_validators as usize + 1)
                .unwrap()
        );
        assert_eq!(state.justifications_roots[2], B256::repeat_byte(2));
        assert!(
            state
                .justifications_validators
                .get(2 * state.config.num_validators as usize + 2)
                .unwrap()
        );
    }

    #[test]
    fn set_justifications_correct_flattened_size() {
        let mut state = LeanState::new(10, 0);
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // Test with a single root
        let root0 = B256::repeat_byte(0);
        let bitlist0 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();

        justifications.insert(root0, bitlist0);

        state.set_justifications(justifications.clone()).unwrap();
        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(
            state.justifications_validators.len(),
            state.config.num_validators as usize
        );

        // Test with 2 roots
        let root1 = B256::repeat_byte(1);
        let bitlist1 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();

        justifications.insert(root1, bitlist1);
        state.set_justifications(justifications).unwrap();
        assert_eq!(state.justifications_roots.len(), 2);
        assert_eq!(
            state.justifications_validators.len(),
            2 * state.config.num_validators as usize
        );
    }

    #[test]
    fn set_justifications_invalid_length() {
        let mut state = LeanState::new(10, 0);
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();
        let invalid_length = state.config.num_validators as usize - 1;

        // root0 voted by validator0
        let root0 = B256::repeat_byte(0);
        let bitlist0 = BitList::<U4096>::with_capacity(invalid_length).unwrap();
        justifications.insert(root0, bitlist0);

        let result = state.set_justifications(justifications);
        assert!(result.is_err());
    }

    #[test]
    fn set_justifications_roundtrip_empty() {
        let mut state = LeanState::new(10, 0);
        let justifications = HashMap::<B256, BitList<U4096>>::new();

        // Set empty justifications to state
        state.set_justifications(justifications.clone()).unwrap();

        // Get justifications back from state
        let reconstructed = state.get_justifications().unwrap();

        // Verify roundtrip equality
        assert_eq!(reconstructed, justifications);
    }

    #[test]
    fn set_justifications_roundtrip_single_root() {
        let mut state = LeanState::new(10, 0);
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // root0 voted by validator 0
        let root0 = B256::repeat_byte(1);
        let mut bitlist0 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();
        bitlist0.set(0, true).unwrap();

        justifications.insert(root0, bitlist0);

        // Set justifications to state
        state.set_justifications(justifications.clone()).unwrap();

        // Get justifications back from state
        let reconstructed = state.get_justifications().unwrap();

        // Verify roundtrip equality
        assert_eq!(reconstructed, justifications);
    }

    #[test]
    fn set_justifications_roundtrip_multiple_roots() {
        let mut state = LeanState::new(10, 0);
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // root0 voted by validator 0
        let root0 = B256::repeat_byte(1);
        let mut bitlist0 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();
        bitlist0.set(0, true).unwrap();

        // root1 voted by validator 1 and 2
        let root1 = B256::repeat_byte(2);
        let mut bitlist1 =
            BitList::<U4096>::with_capacity(state.config.num_validators as usize).unwrap();
        bitlist1.set(1, true).unwrap();
        bitlist1.set(2, true).unwrap();

        justifications.insert(root0, bitlist0);
        justifications.insert(root1, bitlist1);

        // Set justifications to state
        state.set_justifications(justifications.clone()).unwrap();

        // Get justifications back from state
        let reconstructed = state.get_justifications().unwrap();

        // Verify roundtrip equality
        assert_eq!(reconstructed, justifications);
    }

    #[test]
    fn generate_genesis() {
        let config = Config {
            num_validators: 10,
            genesis_time: 0,
        };

        let state = LeanState::new(config.num_validators, config.genesis_time);

        // Config in state should match the input.
        assert_eq!(state.config, config);

        // Slot should start at 0.
        assert_eq!(state.slot, 0);

        // Body root must commit to an empty body at genesis.
        assert_eq!(
            state.latest_block_header.body_root,
            BlockBody::default().tree_hash_root()
        );

        // History and justifications must be empty initially.
        assert_eq!(state.historical_block_hashes.len(), 0);
        assert_eq!(state.justified_slots.len(), 0);
        assert_eq!(state.justifications_roots.len(), 0);
        assert_eq!(state.justifications_validators.num_set_bits(), 0);
    }

    #[test]
    fn process_slot() {
        let mut genesis_state = LeanState::new(10, 0);

        assert_eq!(genesis_state.latest_block_header.state_root, B256::ZERO);

        // Capture the hash of the pre-slot state
        let expected_root = genesis_state.tree_hash_root();

        // Process one slot; this should backfill the header's state_root
        genesis_state.process_slot().unwrap();
        assert_eq!(genesis_state.latest_block_header.state_root, expected_root);

        // Re-processing the slot should be a no-op for the state_root
        genesis_state.process_slot().unwrap();
        assert_eq!(genesis_state.latest_block_header.state_root, expected_root);
    }

    #[test]
    fn process_slots() {
        let mut genesis_state = LeanState::new(10, 0);

        // Choose a future slot target
        let target_slot = 5;

        // Capture the genesis state root before processing
        let expected_root = genesis_state.tree_hash_root();

        // Advance across empty slots to the target
        genesis_state.process_slots(target_slot).unwrap();

        // The state's slot should equal the target
        assert_eq!(genesis_state.slot, target_slot);

        // The header state_root should reflect the genesis state's root
        assert_eq!(genesis_state.latest_block_header.state_root, expected_root);

        // Rewinding is invalid; expect an error
        let result = genesis_state.process_slots(4);
        assert!(result.is_err());
    }

    #[test]
    fn process_block_header_valid() {
        let mut genesis_state = LeanState::new(10, 0);

        genesis_state.process_slots(1).unwrap();

        let genesis_header_root = genesis_state.latest_block_header.tree_hash_root();

        let block = Block {
            slot: genesis_state.slot,
            proposer_index: genesis_state.slot % genesis_state.config.num_validators,
            parent_root: genesis_header_root,
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };

        genesis_state.process_block_header(&block).unwrap();

        // The parent (genesis) becomes both finalized and justified
        assert_eq!(genesis_state.latest_finalized.root, genesis_header_root);
        assert_eq!(genesis_state.latest_justified.root, genesis_header_root);

        // History should include the parent's root at index 0
        assert_eq!(genesis_state.historical_block_hashes.len(), 1);
        assert_eq!(
            genesis_state.historical_block_hashes[0],
            genesis_header_root
        );

        // Slot 0 should be marked justified
        assert_eq!(genesis_state.justified_slots.len(), 1);
        assert!(genesis_state.justified_slots.get(0).unwrap_or(false));

        // Latest header now reflects the processed block's header content
        assert_eq!(genesis_state.latest_block_header.slot, block.slot);
        assert_eq!(
            genesis_state.latest_block_header.parent_root,
            block.parent_root
        );

        // state_root remains zero until the next process_slot call
        assert_eq!(genesis_state.latest_block_header.state_root, B256::ZERO);
    }

    #[test]
    fn process_block_header_invalid_slot() {
        let mut genesis_state = LeanState::new(10, 0);

        // Move to slot 1
        genesis_state.process_slots(1).unwrap();

        let parent_root = genesis_state.latest_block_header.tree_hash_root();

        // Block with wrong slot (2 instead of 1)
        let block = Block {
            slot: 2,
            proposer_index: 1,
            parent_root,
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };

        let result = genesis_state.process_block_header(&block);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Block slot number does not match state slot number")
        );
    }

    #[test]
    fn process_block_header_invalid_proposer() {
        let mut genesis_state = LeanState::new(10, 0);

        // Move to slot 1
        genesis_state.process_slots(1).unwrap();

        let parent_root = genesis_state.latest_block_header.tree_hash_root();

        // Block with wrong proposer (2 instead of 1)
        let block = Block {
            slot: 1,
            proposer_index: 2,
            parent_root,
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };

        let result = genesis_state.process_block_header(&block);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Block proposer index does not match the expected proposer index")
        );
    }

    #[test]
    fn process_block_header_invalid_parent_root() {
        let mut genesis_state = LeanState::new(10, 0);

        // Move to slot 1
        genesis_state.process_slots(1).unwrap();

        // Block with wrong parent root
        let block = Block {
            slot: 1,
            proposer_index: 1,
            parent_root: B256::repeat_byte(0xde),
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };

        let result = genesis_state.process_block_header(&block);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Block parent root does not match latest block header root")
        );
    }

    #[test]
    fn state_transition_full() {
        let genesis_state = LeanState::new(10, 0);

        // Manually compute the post-state result by processing slots first
        let mut state_at_slot_1 = genesis_state.clone();
        state_at_slot_1.process_slots(1).unwrap();

        // Now get the parent root after slot processing
        let parent_root = state_at_slot_1.latest_block_header.tree_hash_root();

        // Build a valid signed block for slot 1
        let block = Block {
            slot: 1,
            proposer_index: 1,
            parent_root,
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };

        // Process the block to get expected state
        let mut expected_state = state_at_slot_1.clone();
        expected_state.process_block(&block).unwrap();

        // Create a block with the correct state root
        let block_with_correct_root = Block {
            slot: 1,
            proposer_index: 1,
            parent_root,
            state_root: expected_state.tree_hash_root(),
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };

        let signed_block = SignedBlock {
            message: block_with_correct_root.clone(),
            signature: Default::default(),
        };

        // Run state transition from genesis
        let mut state = genesis_state.clone();
        state.state_transition(&signed_block, true, true).unwrap();

        // The result must match the expected state
        assert_eq!(state, expected_state);

        // Invalid signatures must cause error
        let mut state2 = genesis_state.clone();
        let result = state2.state_transition(&signed_block, false, true);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Signatures are not valid")
        );

        // Wrong state_root must cause error
        let block_with_bad_root = Block {
            slot: 1,
            proposer_index: 1,
            parent_root,
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };
        let signed_block_with_bad_root = SignedBlock {
            message: block_with_bad_root,
            signature: Default::default(),
        };

        let mut state3 = genesis_state.clone();
        let result = state3.state_transition(&signed_block_with_bad_root, true, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("state root"));
    }
}
