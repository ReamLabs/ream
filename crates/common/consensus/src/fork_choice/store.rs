use alloy_primitives::{map::HashMap, B256};
use serde::{Deserialize, Serialize};

use super::{
    helpers::{constants::GENESIS_SLOT, misc::compute_epoch_at_slot},
    latest_message::LatestMessage,
};
use crate::{
    checkpoint::Checkpoint,
    deneb::{beacon_block::BeaconBlock, beacon_state::BeaconState},
    helpers::compute_start_slot_at_epoch,
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
        let block = self.blocks.get(&root).unwrap();
        if block.slot > slot {
            self.get_ancestor(root, slot)
        } else {
            root
        }
    }
    pub fn get_checkpoint_block(&self, root: B256, epoch: u64) -> B256 {
        let epoch_first_slot = compute_start_slot_at_epoch(epoch);
        self.get_ancestor(root, epoch_first_slot)
    }
}
