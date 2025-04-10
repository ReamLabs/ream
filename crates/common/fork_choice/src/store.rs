use alloy_primitives::{B256, map::HashMap};
use anyhow::anyhow;
use ream_consensus::{
    checkpoint::Checkpoint,
    constants::{
        GENESIS_EPOCH, GENESIS_SLOT, INTERVALS_PER_SLOT, SECONDS_PER_SLOT, SLOTS_PER_EPOCH,
    },
    deneb::{beacon_block::BeaconBlock, beacon_state::BeaconState},
    fork_choice::latest_message::LatestMessage,
    helpers::{calculate_committee_fraction, get_total_active_balance},
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch, is_shuffling_stable},
};
use serde::{Deserialize, Serialize};

use crate::constants::{
    PROPOSER_SCORE_BOOST, REORG_HEAD_WEIGHT_THRESHOLD, REORG_MAX_EPOCHS_SINCE_FINALIZATION,
    REORG_PARENT_WEIGHT_THRESHOLD,
};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub time: u64,
    pub genesis_time: u64,
    pub justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,
    pub unrealized_justified_checkpoint: Checkpoint,
    pub unrealized_finalized_checkpoint: Checkpoint,
    pub proposer_boost_root: B256,
    pub equivocating_indices: Vec<u64>,
    pub blocks: HashMap<B256, BeaconBlock>,
    pub block_states: HashMap<B256, BeaconState>,
    pub block_timeliness: HashMap<B256, bool>,
    pub checkpoint_states: HashMap<Checkpoint, BeaconState>,
    pub latest_messages: HashMap<u64, LatestMessage>,
    pub unrealized_justifications: HashMap<B256, Checkpoint>,
}

impl Store {
    pub fn is_previous_epoch_justified(&self) -> bool {
        let current_epoch = self.get_current_store_epoch();
        self.justified_checkpoint.epoch + 1 == current_epoch
    }

    pub fn get_current_store_epoch(&self) -> u64 {
        compute_epoch_at_slot(self.get_current_slot())
    }

    pub fn get_current_slot(&self) -> u64 {
        GENESIS_SLOT + self.get_slots_since_genesis()
    }

    pub fn get_slots_since_genesis(&self) -> u64 {
        (self.time - self.genesis_time) / SECONDS_PER_SLOT
    }

    pub fn get_ancestor(&self, root: B256, slot: u64) -> anyhow::Result<B256> {
        let block = self
            .blocks
            .get(&root)
            .ok_or(anyhow!("Failed to find root in blocks"))?;
        if block.slot > slot {
            self.get_ancestor(block.parent_root, slot)
        } else {
            Ok(root)
        }
    }

    pub fn get_checkpoint_block(&self, root: B256, epoch: u64) -> anyhow::Result<B256> {
        let epoch_first_slot = compute_start_slot_at_epoch(epoch);
        self.get_ancestor(root, epoch_first_slot)
    }

    pub fn filter_block_tree(
        &self,
        block_root: B256,
        blocks: &mut HashMap<B256, BeaconBlock>,
    ) -> anyhow::Result<bool> {
        let block = &self.blocks[&block_root];

        let children: Vec<B256> = self
            .blocks
            .keys()
            .filter(|&root| self.blocks[root].parent_root == block_root)
            .cloned()
            .collect();

        if !children.is_empty() {
            let filter_results = children
                .iter()
                .map(|child| self.filter_block_tree(*child, blocks))
                .collect::<anyhow::Result<Vec<_>>>()?;

            if filter_results.iter().any(|&result| result) {
                blocks.insert(block_root, block.clone());
                return Ok(true);
            }
            return Ok(false);
        }

        let current_epoch = self.get_current_store_epoch();
        let voting_source = self.get_voting_source(block_root);

        let correct_justified = self.justified_checkpoint.epoch == GENESIS_EPOCH || {
            voting_source.epoch == self.justified_checkpoint.epoch
                || voting_source.epoch + 2 >= current_epoch
        };

        let finalized_checkpoint_block =
            self.get_checkpoint_block(block_root, self.finalized_checkpoint.epoch)?;

        let correct_finalized = self.finalized_checkpoint.epoch == GENESIS_EPOCH
            || self.finalized_checkpoint.root == finalized_checkpoint_block;

        if correct_justified && correct_finalized {
            blocks.insert(block_root, block.clone());
            return Ok(true);
        }

        Ok(false)
    }

    pub fn update_checkpoints(
        &mut self,
        justified_checkpoint: Checkpoint,
        finalized_checkpoint: Checkpoint,
    ) {
        if justified_checkpoint.epoch > self.justified_checkpoint.epoch {
            self.justified_checkpoint = justified_checkpoint;
        }

        if finalized_checkpoint.epoch > self.finalized_checkpoint.epoch {
            self.finalized_checkpoint = finalized_checkpoint;
        }
    }

    pub fn update_unrealized_checkpoints(
        &mut self,
        unrealized_justified_checkpoint: Checkpoint,
        unrealized_finalized_checkpoint: Checkpoint,
    ) {
        if unrealized_justified_checkpoint.epoch > self.unrealized_justified_checkpoint.epoch {
            self.unrealized_justified_checkpoint = unrealized_justified_checkpoint;
        }

        if unrealized_finalized_checkpoint.epoch > self.unrealized_finalized_checkpoint.epoch {
            self.unrealized_finalized_checkpoint = unrealized_finalized_checkpoint;
        }
    }

    // Helper functions
    pub fn is_head_late(&self, head_root: B256) -> bool {
        !self.block_timeliness.get(&head_root).unwrap_or(&true)
    }

    pub fn is_ffg_competitive(&self, head_root: B256, parent_root: B256) -> bool {
        self.unrealized_justifications.get(&head_root)
            == self.unrealized_justifications.get(&parent_root)
    }

    pub fn is_proposing_on_time(&self) -> bool {
        let time_into_slot = (self.time - self.genesis_time) % SECONDS_PER_SLOT;
        let proposer_reorg_cutoff = SECONDS_PER_SLOT / INTERVALS_PER_SLOT / 2;
        time_into_slot <= proposer_reorg_cutoff
    }

    pub fn is_finalization_ok(&self, slot: u64) -> bool {
        let epochs_since_finalization =
            compute_epoch_at_slot(slot) - self.finalized_checkpoint.epoch;
        epochs_since_finalization <= REORG_MAX_EPOCHS_SINCE_FINALIZATION
    }

    pub fn get_proposer_score(&self) -> anyhow::Result<u64> {
        let justified_checkpoint_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .ok_or(anyhow!("Failed to find checkpoint in checkpoint states"))?;
        let committee_weight =
            get_total_active_balance(justified_checkpoint_state) / SLOTS_PER_EPOCH;
        Ok((committee_weight * PROPOSER_SCORE_BOOST) / 100)
    }

    pub fn get_weight(&self, root: B256) -> anyhow::Result<u64> {
        let state = &self.checkpoint_states[&self.justified_checkpoint];

        let unslashed_and_active_indices: Vec<u64> = state
            .get_active_validator_indices(state.get_current_epoch())
            .into_iter()
            .filter(|&i| !state.validators[i as usize].slashed)
            .collect();

        let mut attestation_score: u64 = 0;
        for index in unslashed_and_active_indices {
            if self.latest_messages.contains_key(&index)
                && !self.equivocating_indices.contains(&index)
                && self.get_ancestor(self.latest_messages[&index].root, self.blocks[&root].slot)?
                    == root
            {
                attestation_score += state.validators[index as usize].effective_balance;
            }
        }

        if self.proposer_boost_root == B256::ZERO {
            return Ok(attestation_score);
        }

        let mut proposer_score: u64 = 0;
        if self.get_ancestor(self.proposer_boost_root, self.blocks[&root].slot)? == root {
            proposer_score = self.get_proposer_score()?;
        }

        Ok(attestation_score + proposer_score)
    }

    pub fn get_voting_source(&self, block_root: B256) -> Checkpoint {
        let block = &self.blocks[&block_root];

        let current_epoch = self.get_current_store_epoch();
        let block_epoch = compute_epoch_at_slot(block.slot);

        if current_epoch > block_epoch {
            self.unrealized_justifications[&block_root]
        } else {
            let head_state = &self.block_states[&block_root];
            head_state.current_justified_checkpoint
        }
    }

    pub fn is_head_weak(&self, head_root: B256) -> anyhow::Result<bool> {
        let justified_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .ok_or(anyhow!("Justified checkpoint must exist in the store"))?;

        let reorg_threshold =
            calculate_committee_fraction(justified_state, REORG_HEAD_WEIGHT_THRESHOLD);
        let head_weight = self.get_weight(head_root)?;

        Ok(head_weight < reorg_threshold)
    }

    pub fn is_parent_strong(&self, parent_root: B256) -> anyhow::Result<bool> {
        let justified_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .ok_or(anyhow!("Justified checkpoint must exist in the store"))?;

        let parent_threshold =
            calculate_committee_fraction(justified_state, REORG_PARENT_WEIGHT_THRESHOLD);
        let parent_weight = self.get_weight(parent_root)?;

        Ok(parent_weight > parent_threshold)
    }

    pub fn get_proposer_head(&self, head_root: B256, slot: u64) -> anyhow::Result<B256> {
        let head_block = self
            .blocks
            .get(&head_root)
            .ok_or(anyhow!("Head block must exist"))?;
        let parent_root = head_block.parent_root;
        let parent_block = self
            .blocks
            .get(&parent_root)
            .ok_or(anyhow!("Parent block must exist"))?;

        let head_late = self.is_head_late(head_root);

        let shuffling_stable = is_shuffling_stable(slot);

        let ffg_competitive = self.is_ffg_competitive(head_root, parent_root);

        let finalization_ok = self.is_finalization_ok(slot);

        let proposing_on_time = self.is_proposing_on_time();

        let parent_slot_ok = parent_block.slot + 1 == head_block.slot;
        let current_time_ok = head_block.slot + 1 == slot;
        let single_slot_reorg = parent_slot_ok && current_time_ok;

        assert!(self.proposer_boost_root != head_root); // Ensure boost has worn off
        let head_weak = self.is_head_weak(head_root)?;

        let parent_strong = self.is_parent_strong(parent_root)?;

        if head_late
            && shuffling_stable
            && ffg_competitive
            && finalization_ok
            && proposing_on_time
            && single_slot_reorg
            && head_weak
            && parent_strong
        {
            Ok(parent_root)
        } else {
            Ok(head_root)
        }
    }
}
