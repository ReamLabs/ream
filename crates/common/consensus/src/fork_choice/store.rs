use alloy_primitives::{aliases::B32, map::HashMap};
use serde::{Deserialize, Serialize};

use super::{
    helpers::{constants::GENESIS_SLOT, misc::compute_epoch_at_slot},
    latest_message::LatestMessage,
};
use crate::{
    checkpoint::Checkpoint,
    deneb::{beacon_block::BeaconBlock, beacon_state::BeaconState},
};
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Store {
    pub time: u64,
    pub genesis_time: u64,
    pub justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,
    pub unrealized_justified_checkpoint: Checkpoint,
    pub unrealized_finalized_checkpoint: Checkpoint,
    pub proposer_boost_root: B32,
    pub equivocating_indices: Vec<u64>,
    pub blocks: HashMap<B32, BeaconBlock>,
    pub block_states: HashMap<B32, BeaconState>,
    pub block_timeliness: HashMap<B32, bool>,
    pub checkpoint_states: HashMap<Checkpoint, BeaconState>,
    pub latest_messages: HashMap<u64, LatestMessage>,
    pub unrealized_justifications: HashMap<B32, Checkpoint>,
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
}
