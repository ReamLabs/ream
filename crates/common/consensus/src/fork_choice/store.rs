use alloy_primitives::{B256, map::HashMap};
use serde::{Deserialize, Serialize};

use super::{
    helpers::constants::{
        GENESIS_EPOCH, GENESIS_SLOT, INTERVALS_PER_SLOT, REORG_HEAD_WEIGHT_THRESHOLD,
        REORG_MAX_EPOCHS_SINCE_FINALIZATION, REORG_PARENT_WEIGHT_THRESHOLD, SECONDS_PER_SLOT,
    },
    latest_message::LatestMessage,
};
use crate::{
    checkpoint::Checkpoint,
    deneb::{beacon_block::BeaconBlock, beacon_state::BeaconState},
    helpers::{calculate_committee_fraction, get_voting_source, get_weight},
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch, is_shuffling_stable},
};
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
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
        let current_epoch = compute_epoch_at_slot(self.get_current_store_slot());
        self.justified_checkpoint.epoch + 1 == current_epoch
    }

    pub fn get_current_store_slot(&self) -> u64 {
        compute_epoch_at_slot(self.get_current_slot())
    }

    pub fn get_current_slot(&self) -> u64 {
        GENESIS_SLOT + self.get_slots_since_genesis()
    }

    pub fn get_slots_since_genesis(&self) -> u64 {
        self.time - self.genesis_time
    }

    pub fn get_ancestor(&self, root: B256, slot: u64) -> B256 {
        let block = self
            .blocks
            .get(&root)
            .expect("Failed to find root in blocks");
        if block.slot > slot {
            self.get_ancestor(block.parent_root, slot)
        } else {
            root
        }
    }

    pub fn get_checkpoint_block(&self, root: B256, epoch: u64) -> B256 {
        let epoch_first_slot = compute_start_slot_at_epoch(epoch);
        self.get_ancestor(root, epoch_first_slot)
    }

    pub fn filter_block_tree(
        &self,
        block_root: B256,
        blocks: &mut HashMap<B256, BeaconBlock>,
    ) -> bool {
        let block = &self.blocks[&block_root];

        let children: Vec<B256> = self
            .blocks
            .keys()
            .filter(|&root| self.blocks[root].parent_root == block_root)
            .cloned()
            .collect();

        if !children.is_empty() {
            let filter_results: Vec<bool> = children
                .iter()
                .map(|child| self.filter_block_tree(*child, blocks))
                .collect();

            if filter_results.iter().any(|&result| result) {
                blocks.insert(block_root, block.clone());
                return true;
            }
            return false;
        }

        let current_epoch = compute_epoch_at_slot(self.get_current_slot());
        let voting_source = get_voting_source(self, block_root);

        let correct_justified = self.justified_checkpoint.epoch == GENESIS_EPOCH || {
            voting_source.epoch == self.justified_checkpoint.epoch
                || voting_source.epoch + 2 >= current_epoch
        };

        let finalized_checkpoint_block =
            self.get_checkpoint_block(block_root, self.finalized_checkpoint.epoch);

        let correct_finalized = self.finalized_checkpoint.epoch == GENESIS_EPOCH
            || self.finalized_checkpoint.root == finalized_checkpoint_block;

        if correct_justified && correct_finalized {
            blocks.insert(block_root, block.clone());
            return true;
        }

        false
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

    pub fn is_head_weak(&self, head_root: B256) -> bool {
        let justified_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .expect("Justified checkpoint must exist in the store");

        let reorg_threshold =
            calculate_committee_fraction(justified_state.clone(), REORG_HEAD_WEIGHT_THRESHOLD);
        let head_weight = get_weight(self.clone(), head_root);

        head_weight < reorg_threshold
    }

    pub fn is_parent_strong(&self, parent_root: B256) -> bool {
        let justified_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .expect("Justified checkpoint must exist in the store");

        let parent_threshold =
            calculate_committee_fraction(justified_state.clone(), REORG_PARENT_WEIGHT_THRESHOLD);
        let parent_weight = get_weight(self.clone(), parent_root);

        parent_weight > parent_threshold
    }

    pub fn get_proposer_head(&self, head_root: B256, slot: u64) -> B256 {
        let head_block = self.blocks.get(&head_root).expect("Head block must exist");
        let parent_root = head_block.parent_root;
        let parent_block = self
            .blocks
            .get(&parent_root)
            .expect("Parent block must exist");

        let head_late = self.is_head_late(head_root);

        let shuffling_stable = is_shuffling_stable(slot);

        let ffg_competitive = self.is_ffg_competitive(head_root, parent_root);

        let finalization_ok = self.is_finalization_ok(slot);

        let proposing_on_time = self.is_proposing_on_time();

        let parent_slot_ok = parent_block.slot + 1 == head_block.slot;
        let current_time_ok = head_block.slot + 1 == slot;
        let single_slot_reorg = parent_slot_ok && current_time_ok;

        assert!(self.proposer_boost_root != head_root); // Ensure boost has worn off
        let head_weak = self.is_head_weak(head_root);

        let parent_strong = self.is_parent_strong(parent_root);

        if head_late
            && shuffling_stable
            && ffg_competitive
            && finalization_ok
            && proposing_on_time
            && single_slot_reorg
            && head_weak
            && parent_strong
        {
            parent_root
        } else {
            head_root
        }
    }
}
