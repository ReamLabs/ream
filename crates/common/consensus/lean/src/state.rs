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
    attestation::Attestation,
    block::{Block, BlockBody, BlockHeader},
    checkpoint::Checkpoint,
    config::Config,
    is_justifiable_slot,
    validator::{Validator, is_proposer},
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
    pub fn generate_genesis(genesis_time: u64, validators: Option<Vec<Validator>>) -> LeanState {
        LeanState {
            config: Config { genesis_time },
            slot: 0,
            latest_block_header: BlockHeader {
                slot: 0,
                proposer_index: 0,
                parent_root: B256::ZERO,
                state_root: B256::ZERO,
                body_root: BlockBody {
                    attestations: Default::default(),
                }
                .tree_hash_root(),
            },

            latest_justified: Checkpoint::default(),
            latest_finalized: Checkpoint::default(),

            historical_block_hashes: VariableList::empty(),
            justified_slots: BitList::with_capacity(0)
                .expect("Failed to initialize an empty BitList"),

            validators: VariableList::from(validators.unwrap_or_default()),

            justifications_roots: VariableList::empty(),
            justifications_validators: BitList::with_capacity(0)
                .expect("Failed to initialize an empty BitList"),
        }
    }

    /// Returns a map of `root -> justifications` constructed from the
    /// flattened data in the state.
    pub fn get_justifications(&self) -> anyhow::Result<HashMap<B256, BitList<U4096>>> {
        let mut justifications = HashMap::new();

        if self.justifications_roots.is_empty() {
            return Ok(justifications);
        }

        let validator_count = self.validators.len();

        let flat_votes = self.justifications_validators.iter().collect::<Vec<_>>();

        for (i, root) in self.justifications_roots.iter().enumerate() {
            let start_index = i * validator_count;
            let end_index = start_index + validator_count;
            let vote_slice = &flat_votes
                .get(start_index..end_index)
                .expect("Could not get indexs");

            let mut new_bitlist = BitList::with_capacity(validator_count)
                .map_err(|err| anyhow!("Failed to create BitList for justifications: {err:?}"))?;

            for (validator_index, &bit) in vote_slice.iter().enumerate() {
                new_bitlist
                    .set(validator_index, bit)
                    .map_err(|err| anyhow!("Failed to set justification: {err:?}"))?;
            }

            justifications.insert(*root, new_bitlist);
        }
        Ok(justifications)
    }

    /// Saves a map of `root -> justifications` back into the state's flattened
    /// data structure.
    pub fn with_justifications(
        &mut self,
        justifications: HashMap<B256, BitList<U4096>>,
    ) -> anyhow::Result<()> {
        let mut roots_list = VariableList::<B256, U262144>::empty();
        let mut votes_list: Vec<bool> = Vec::new();

        for root in justifications.keys().sorted() {
            let votes = justifications
                .get(root)
                .ok_or_else(|| anyhow!("Root {root} not found in justifications"))?;
            ensure!(
                votes.len() == self.validators.len(),
                "Vote list for root {root} has incorrect length expected: {}, got: {}",
                votes.len(),
                self.validators.len(),
            );

            roots_list
                .push(*root)
                .map_err(|err| anyhow!("Could not append root: {err:?}"))?;
            votes.iter().for_each(|vote| votes_list.push(vote));
        }

        let mut justifications_validators =
            BitList::with_capacity(justifications.len() * self.validators.len()).map_err(
                |err| anyhow!("Failed to create BitList for justifications_validators: {err:?}"),
            )?;

        votes_list.iter().enumerate().try_for_each(
            |(index, justification)| -> anyhow::Result<()> {
                justifications_validators
                    .set(index, *justification)
                    .map_err(|err| anyhow!("Failed to set justification bit: {err:?}"))
            },
        )?;

        self.justifications_roots = roots_list;
        self.justifications_validators = justifications_validators;

        Ok(())
    }

    pub fn state_transition(
        &mut self,
        block: &Block,
        valid_signatures: bool,
    ) -> anyhow::Result<()> {
        // Validate signatures if required
        ensure!(valid_signatures, "Signatures are not valid");
        self.process_slots(block.slot)
            .context("failed to process intermediate slots")?;
        self.process_block(block)
            .context("failed to process block")?;

        ensure!(
            block.state_root == self.tree_hash_root(),
            "Invalid block state root"
        );

        Ok(())
    }

    pub fn process_slots(&mut self, target_slot: u64) -> anyhow::Result<()> {
        ensure!(
            self.slot < target_slot,
            "Target slot must be in the future, expectec {} < {target_slot}",
            self.slot,
        );
        while self.slot < target_slot {
            if self.latest_block_header.state_root == B256::ZERO {
                self.latest_block_header.state_root = self.tree_hash_root();
            }
            self.slot += 1;
        }

        Ok(())
    }

    pub fn process_block(&mut self, block: &Block) -> anyhow::Result<()> {
        self.process_block_header(block)?;
        self.process_attestations(&block.body.attestations)?;

        Ok(())
    }

    /// Check if a validator is the proposer for the current slot.
    fn is_proposer(&self, validator_index: u64) -> bool {
        is_proposer(validator_index, self.slot, self.validators.len() as u64)
    }

    /// Validate the block header and update header-linked state.
    fn process_block_header(&mut self, block: &Block) -> anyhow::Result<()> {
        // The block must be for the current slot.
        ensure!(
            block.slot == self.slot,
            "Block slot number does not match state slot number"
        );
        // Block is older than latest header
        ensure!(
            block.slot > self.latest_block_header.slot,
            "Block slot number is not greater than latest block header slot number"
        );
        // The proposer must be the expected validator for this slot.
        ensure!(
            self.is_proposer(block.proposer_index),
            "Block proposer index does not match the expected proposer index"
        );

        // The declared parent must match the hash of the latest block header.
        ensure!(
            block.parent_root == self.latest_block_header.tree_hash_root(),
            "Block parent root does not match latest block header root"
        );

        // Special case: first block after genesis.
        if self.latest_block_header.slot == 0 {
            // block.parent_root is the genesis root
            self.latest_justified.root = block.parent_root;
            self.latest_finalized.root = block.parent_root;
        }

        // now that we can attestations on parent, push it at its correct slot index in the
        // structures
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
        if num_empty_slots > 0 {
            for _ in 0..num_empty_slots {
                self.historical_block_hashes
                    .push(B256::ZERO)
                    .map_err(|err| anyhow!("Failed to prefill historical_block_hashes: {err:?}"))?;
            }
            let length = self.justified_slots.len();
            let new_bitlist = BitList::with_capacity(length + num_empty_slots as usize)
                .map_err(|err| anyhow!("Failed to resize justified_slots BitList: {err:?}"))?;
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

    pub fn process_attestations(&mut self, attestations: &[Attestation]) -> anyhow::Result<()> {
        // get justifications, justified slots and historical block hashes are
        // already up to date as per the processing in process_block_header
        let mut justifications_map = self.get_justifications()?;

        for attestation in attestations {
            // Ignore attestations whose source is not already justified,
            // or whose target is not in the history, or whose target is not a
            // valid justifiable slot
            if !self
                .justified_slots
                .get(attestation.source().slot as usize)
                .map_err(|err| anyhow!("Failed to get justified slot: {err:?}"))?
            {
                info!(
                    reason = "Source slot not justified",
                    source_slot = attestation.source().slot,
                    target_slot = attestation.target().slot,
                    "Skipping attestations by Validator {}",
                    attestation.validator_id,
                );
                continue;
            }

            // This condition is missing in 3sf mini but has been added here because
            // we don't want to re-introduce the target again for remaining attestations if
            // the slot is already justified and its tracking already cleared out
            // from justifications map
            if self
                .justified_slots
                .get(attestation.target().slot as usize)
                .map_err(|err| anyhow!("Failed to get justified slot: {err:?}"))?
            {
                info!(
                    reason = "Target slot already justified",
                    source_slot = attestation.source().slot,
                    target_slot = attestation.target().slot,
                    "Skipping attestations by Validator {}",
                    attestation.validator_id,
                );
                continue;
            }

            if attestation.source().root
                != *self
                    .historical_block_hashes
                    .get(attestation.source().slot as usize)
                    .ok_or(anyhow!("Source slot not found in historical_block_hashes"))?
            {
                info!(
                    reason = "Source block not in historical block hashes",
                    source_slot = attestation.source().slot,
                    target_slot = attestation.target().slot,
                    "Skipping attestations by Validator {}",
                    attestation.validator_id,
                );
                continue;
            }

            if attestation.target().root
                != *self
                    .historical_block_hashes
                    .get(attestation.target().slot as usize)
                    .ok_or(anyhow!("Target slot not found in historical_block_hashes"))?
            {
                info!(
                    reason = "Target block not in historical block hashes",
                    source_slot = attestation.source().slot,
                    target_slot = attestation.target().slot,
                    "Skipping attestations by Validator {}",
                    attestation.validator_id,
                );
                continue;
            }

            if attestation.target().slot <= attestation.source().slot {
                info!(
                    reason = "Target slot not greater than source slot",
                    source_slot = attestation.source().slot,
                    target_slot = attestation.target().slot,
                    "Skipping attestations by Validator {}",
                    attestation.validator_id,
                );
                continue;
            }

            if !is_justifiable_slot(self.latest_finalized.slot, attestation.target().slot) {
                info!(
                    reason = "Target slot not justifiable",
                    source_slot = attestation.source().slot,
                    target_slot = attestation.target().slot,
                    "Skipping attestations by Validator {}",
                    attestation.validator_id,
                );
                continue;
            }

            // Track attempts to justify new hashes
            let justifications = justifications_map
                .entry(attestation.target().root)
                .or_insert(
                    BitList::with_capacity(self.validators.len()).map_err(|err| {
                        anyhow!(
                            "Failed to initialize justification for root {:?}: {err:?}",
                            &attestation.target().root
                        )
                    })?,
                );

            justifications
                .set(attestation.validator_id as usize, true)
                .map_err(|err| {
                    anyhow!(
                        "Failed to set validator {:?}'s justification for root {:?}: {err:?}",
                        attestation.validator_id,
                        &attestation.target().root
                    )
                })?;

            let count = justifications.num_set_bits();

            // If 2/3 attestations for the same new valid hash to justify
            // in 3sf mini this is strict equality, but we have updated it to >=
            // also have modified it from count >= (2 * state.config.num_validators) // 3
            // to prevent integer division which could lead to less than 2/3 of validators
            // justifying specially if the num_validators is low in testing scenarios
            if 3 * count >= (2 * self.validators.len()) {
                self.latest_justified = attestation.target();
                self.justified_slots
                    .set(attestation.target().slot as usize, true)
                    .map_err(|err| {
                        anyhow!(
                            "Failed to set justified slot for slot {}: {err:?}",
                            attestation.target().slot
                        )
                    })?;

                justifications_map.remove(&attestation.target().root);

                info!(
                    slot = self.latest_justified.slot,
                    root = ?self.latest_justified.root,
                    "Justification event",
                );
                set_int_gauge_vec(&JUSTIFIED_SLOT, self.latest_justified.slot as i64, &[]);

                // Finalization: if the target is the next valid justifiable
                // hash after the source
                let is_target_next_valid_justifiable_slot = !((attestation.source().slot + 1)
                    ..attestation.target().slot)
                    .any(|slot| is_justifiable_slot(self.latest_finalized.slot, slot));

                if is_target_next_valid_justifiable_slot {
                    self.latest_finalized = attestation.source();

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
        self.with_justifications(justifications_map)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::attestation::{Attestation, AttestationData};

    #[test]
    fn get_justifications_empty() {
        let state = LeanState::generate_genesis(10, None);

        // Ensure the base state has empty justification lists
        assert_eq!(state.justifications_roots.len(), 0);
        assert_eq!(state.justifications_validators.num_set_bits(), 0);

        let justifications_map = state.get_justifications().unwrap();
        assert_eq!(justifications_map, HashMap::new());
    }

    #[test]
    fn get_justifications_single_root() {
        let num_validators: usize = 3;
        let mut state = LeanState::generate_genesis(
            0,
            Some(Validator::generate_default_validators(num_validators)),
        );
        state.justifications_validators = BitList::with_capacity(num_validators).unwrap();
        let root = B256::repeat_byte(1);

        state.justifications_roots.push(root).unwrap();
        state.justifications_validators.set(1, true).unwrap();

        let mut expected_bitlist = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();
        expected_bitlist.set(1, true).unwrap();

        let mut expected_map = HashMap::<B256, BitList<U4096>>::new();
        expected_map.insert(root, expected_bitlist);

        let justifications_map = state.get_justifications().unwrap();
        assert_eq!(justifications_map, expected_map);
    }

    #[test]
    fn get_justifications_multiple_roots() {
        let num_validators = 3;
        let mut state = LeanState::generate_genesis(
            0,
            Some(Validator::generate_default_validators(num_validators)),
        );
        state.justifications_validators = BitList::with_capacity(num_validators * 3).unwrap();
        let root0 = B256::repeat_byte(0);
        let root1 = B256::repeat_byte(1);
        let root2 = B256::repeat_byte(2);

        // root0 is attestation by validator 0
        state.justifications_roots.push(root0).unwrap();
        state.justifications_validators.set(0, true).unwrap();

        // root1 is attestation by validator 1 and 2
        state.justifications_roots.push(root1).unwrap();
        state
            .justifications_validators
            .set(state.validators.len() + 1, true)
            .unwrap();
        state
            .justifications_validators
            .set(state.validators.len() + 2, true)
            .unwrap();

        // root2 is attestation by none
        state.justifications_roots.push(root2).unwrap();

        // Verify that the reconstructed map is identical to the expected map
        // Because HashMap is not ordered, we need to check root by root
        let justifications = state.get_justifications().unwrap();

        // check root0 attestation by validator 0
        let mut expected_bitlist0 = BitList::with_capacity(state.validators.len()).unwrap();
        expected_bitlist0.set(0, true).unwrap();
        assert_eq!(justifications[&root0], expected_bitlist0);

        // Prepare expected root1 attested by validator 1 and 2
        let mut expected_bitlist1 = BitList::with_capacity(state.validators.len()).unwrap();
        expected_bitlist1.set(1, true).unwrap();
        expected_bitlist1.set(2, true).unwrap();
        assert_eq!(justifications[&root1], expected_bitlist1);

        // Prepare expected root2 attested by none
        let expected_bitlist2 = BitList::with_capacity(state.validators.len()).unwrap();
        assert_eq!(justifications[&root2], expected_bitlist2);

        // Also verify that the number of roots matches
        assert_eq!(justifications.len(), 3);
    }

    #[test]
    fn with_justifications_empty() {
        let mut state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        state.justifications_validators = BitList::with_capacity(state.validators.len()).unwrap();
        state
            .justifications_roots
            .push(B256::repeat_byte(1))
            .unwrap();
        state.justifications_validators.set(0, true).unwrap();

        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(state.justifications_validators.num_set_bits(), 1);

        let justifications = HashMap::<B256, BitList<U4096>>::new();
        state.with_justifications(justifications).unwrap();

        assert_eq!(state.justifications_roots.len(), 0);
        assert_eq!(state.justifications_validators.num_set_bits(), 0);
    }

    #[test]
    fn with_justifications_deterministic_order() {
        let mut state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // root0 attested by validator0
        let root0 = B256::repeat_byte(0);
        let mut bitlist0 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();
        bitlist0.set(0, true).unwrap();

        // root1 attested by validator1
        let root1 = B256::repeat_byte(1);
        let mut bitlist1 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();
        bitlist1.set(1, true).unwrap();

        // root2 attested by validator2
        let root2 = B256::repeat_byte(2);
        let mut bitlist2 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();
        bitlist2.set(2, true).unwrap();

        // Insert unordered: root0, root2, root1
        justifications.insert(root0, bitlist0);
        justifications.insert(root2, bitlist2);
        justifications.insert(root1, bitlist1);

        state.with_justifications(justifications).unwrap();

        assert_eq!(state.justifications_roots[0], B256::repeat_byte(0));
        assert!(state.justifications_validators.get(0).unwrap());
        assert_eq!(state.justifications_roots[1], B256::repeat_byte(1));
        assert!(
            state
                .justifications_validators
                .get(state.validators.len() + 1)
                .unwrap()
        );
        assert_eq!(state.justifications_roots[2], B256::repeat_byte(2));
        assert!(
            state
                .justifications_validators
                .get(2 * state.validators.len() + 2)
                .unwrap()
        );
    }

    #[test]
    fn with_justifications_correct_flattened_size() {
        let mut state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // Test with a single root
        let root0 = B256::repeat_byte(0);
        let bitlist0 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();

        justifications.insert(root0, bitlist0);

        state.with_justifications(justifications.clone()).unwrap();
        assert_eq!(state.justifications_roots.len(), 1);
        assert_eq!(
            state.justifications_validators.len(),
            state.validators.len()
        );

        // Test with 2 roots
        let root1 = B256::repeat_byte(1);
        let bitlist1 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();

        justifications.insert(root1, bitlist1);
        state.with_justifications(justifications).unwrap();
        assert_eq!(state.justifications_roots.len(), 2);
        assert_eq!(
            state.justifications_validators.len(),
            2 * state.validators.len()
        );
    }

    #[test]
    fn with_justifications_invalid_length() {
        let mut state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();
        let invalid_length = state.validators.len() - 1;

        // root0 attested by validator0
        let root0 = B256::repeat_byte(0);
        let bitlist0 = BitList::<U4096>::with_capacity(invalid_length).unwrap();
        justifications.insert(root0, bitlist0);

        let result = state.with_justifications(justifications);
        assert!(result.is_err());
    }

    #[test]
    fn with_justifications_roundtrip_empty() {
        let mut state = LeanState::generate_genesis(10, None);
        let justifications = HashMap::<B256, BitList<U4096>>::new();

        // Set empty justifications to state
        state.with_justifications(justifications.clone()).unwrap();

        // Get justifications back from state
        let reconstructed = state.get_justifications().unwrap();

        // Verify roundtrip equality
        assert_eq!(reconstructed, justifications);
    }

    #[test]
    fn with_justifications_roundtrip_single_root() {
        let mut state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // root0 attested by validator 0
        let root0 = B256::repeat_byte(1);
        let mut bitlist0 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();
        bitlist0.set(0, true).unwrap();

        justifications.insert(root0, bitlist0);

        // Set justifications to state
        state.with_justifications(justifications.clone()).unwrap();

        // Get justifications back from state
        let reconstructed = state.get_justifications().unwrap();

        // Verify roundtrip equality
        assert_eq!(reconstructed, justifications);
    }

    #[test]
    fn with_justifications_roundtrip_multiple_roots() {
        let mut state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut justifications = HashMap::<B256, BitList<U4096>>::new();

        // root0 attested by validator 0
        let root0 = B256::repeat_byte(1);
        let mut bitlist0 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();
        bitlist0.set(0, true).unwrap();

        // root1 attested by validator 1 and 2
        let root1 = B256::repeat_byte(2);
        let mut bitlist1 = BitList::<U4096>::with_capacity(state.validators.len()).unwrap();
        bitlist1.set(1, true).unwrap();
        bitlist1.set(2, true).unwrap();

        justifications.insert(root0, bitlist0);
        justifications.insert(root1, bitlist1);

        // Set justifications to state
        state.with_justifications(justifications.clone()).unwrap();

        // Get justifications back from state
        let reconstructed = state.get_justifications().unwrap();

        // Verify roundtrip equality
        assert_eq!(reconstructed, justifications);
    }

    #[test]
    fn generate_genesis() {
        let config = Config { genesis_time: 0 };

        let state = LeanState::generate_genesis(
            config.genesis_time,
            Some(Validator::generate_default_validators(10)),
        );

        // Config in state should match the input.
        assert_eq!(state.config, config);

        // Slot should start at 0.
        assert_eq!(state.slot, 0);

        // Body root must commit to an empty body at genesis.
        assert_eq!(
            state.latest_block_header.body_root,
            BlockBody {
                attestations: Default::default()
            }
            .tree_hash_root()
        );

        // History and justifications must be empty initially.
        assert_eq!(state.historical_block_hashes.len(), 0);
        assert_eq!(state.justified_slots.len(), 0);
        assert_eq!(state.justifications_roots.len(), 0);
        assert_eq!(state.justifications_validators.num_set_bits(), 0);
    }

    #[test]
    fn process_slots() {
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));

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
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));

        genesis_state.process_slots(1).unwrap();

        let genesis_header_root = genesis_state.latest_block_header.tree_hash_root();

        let block = Block {
            slot: genesis_state.slot,
            proposer_index: genesis_state.slot % (genesis_state.validators.len() as u64),
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
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));

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
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));

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
        let result_error_string = result.unwrap_err().to_string();
        assert!(
            result_error_string
                .contains("Block proposer index does not match the expected proposer index"),
            "unexpeceted result: {result_error_string}"
        );
    }

    #[test]
    fn process_block_header_invalid_parent_root() {
        let mut genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));

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
    fn process_attestations_justification_and_finalization() {
        let genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));
        let mut state = genesis_state.clone();

        // Move to slot 1 to allow producing a block there.
        state.process_slots(1).unwrap();

        // Create and process the block at slot 1.
        let block_1 = Block {
            slot: 1,
            proposer_index: 1,
            parent_root: state.latest_block_header.tree_hash_root(),
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };
        state.process_block(&block_1).unwrap();

        // Move to slot 4 and produce/process a block.
        state.process_slots(4).unwrap();
        let block_4 = Block {
            slot: 4,
            proposer_index: 4,
            parent_root: state.latest_block_header.tree_hash_root(),
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };
        state.process_block(&block_4).unwrap();

        // Advance to slot 5 so the header at slot 4 caches its state root.
        state.process_slots(5).unwrap();

        // Process a block at slot 5 to push block_4's root into historical_block_hashes.
        // This is required by the our implementation based off 3SF-mini which validates that target
        // roots exist in historical_block_hashes before accepting attestations
        // This validation does not exist in leanSpec so the test passes without processing block 5
        // We deviate from the leanSpec in this test and process block 5 before testing
        // process_attestations for slot 4
        let block_5 = Block {
            slot: 5,
            proposer_index: 5,
            parent_root: state.latest_block_header.tree_hash_root(),
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: VariableList::empty(),
            },
        };
        state.process_block(&block_5).unwrap();

        // Define source (genesis) and target (slot 4) checkpoints for voting.
        let genesis_checkpoint = Checkpoint {
            root: state.historical_block_hashes[0], // Canonical root for slot 0
            slot: 0,
        };
        let checkpoint_4 = Checkpoint {
            root: state.historical_block_hashes[4], // Root of the block at slot 4
            slot: 4,
        };

        // Create 7 attestations from distinct validators (indices 0..6) to reach ≥2/3.
        let mut attestations_for_4 = VariableList::<Attestation, U4096>::empty();
        for i in 0..7 {
            let attestations = Attestation {
                validator_id: i,
                data: AttestationData {
                    slot: 4,
                    head: checkpoint_4,
                    target: checkpoint_4,
                    source: genesis_checkpoint,
                },
            };
            attestations_for_4.push(attestations).unwrap();
        }

        // Process attestations directly; mutates state in place.
        state.process_attestations(&attestations_for_4).unwrap();

        // The target (slot 4) should now be justified.
        assert_eq!(state.latest_justified, checkpoint_4);
        // The justified bit for slot 4 must be set.
        assert!(state.justified_slots.get(4).unwrap_or(false));
        // Since no other justifiable slot exists between 0 and 4, genesis is finalized.
        assert_eq!(state.latest_finalized, genesis_checkpoint);
        // The per-root attestations tracker for the justified target has been cleared.
        let justifications = state.get_justifications().unwrap();
        assert!(!justifications.contains_key(&checkpoint_4.root));
    }

    #[test]
    fn state_transition_full() {
        let genesis_state =
            LeanState::generate_genesis(0, Some(Validator::generate_default_validators(10)));

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

        // Run state transition from genesis
        let mut state = genesis_state.clone();
        state
            .state_transition(&block_with_correct_root, true)
            .unwrap();

        // The result must match the expected state
        assert_eq!(state, expected_state);

        // Invalid signatures must cause error
        let mut state_2 = genesis_state.clone();
        let result = state_2.state_transition(&block_with_correct_root, false);
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

        let mut state_3 = genesis_state.clone();
        let result = state_3.state_transition(&block_with_bad_root, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("state root"));
    }
}
