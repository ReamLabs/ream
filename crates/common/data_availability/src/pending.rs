use std::collections::HashSet;

use ream_consensus_beacon::electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState};

#[derive(Debug, Clone)]
pub struct PendingBlock<State = BeaconState> {
    pub signed_block: SignedBeaconBlock,
    pub post_state: State,
}

#[derive(Debug, Clone)]
pub struct PendingAvailability<State = BeaconState> {
    pub pending_block: Option<PendingBlock<State>>,
    pub received_columns: HashSet<u64>,
    pub slot: u64,
}

impl<State> Default for PendingAvailability<State> {
    fn default() -> Self {
        Self {
            pending_block: None,
            received_columns: HashSet::new(),
            slot: 0,
        }
    }
}
