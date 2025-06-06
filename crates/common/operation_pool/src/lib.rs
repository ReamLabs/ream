use std::collections::HashMap;

use parking_lot::RwLock;
use ream_consensus::{electra::beacon_state::BeaconState, voluntary_exit::SignedVoluntaryExit};

#[derive(Debug, Default)]
pub struct OperationPool {
    signed_voluntary_exits: RwLock<HashMap<u64, SignedVoluntaryExit>>,
}

impl OperationPool {
    pub fn insert_signed_voluntary_exit(&self, signed_voluntary_exit: SignedVoluntaryExit) {
        let mut signed_voluntary_exits = self.signed_voluntary_exits.write();
        signed_voluntary_exits.insert(
            signed_voluntary_exit.message.validator_index,
            signed_voluntary_exit,
        );
    }

    pub fn get_signed_voluntary_exits(&self) -> Vec<SignedVoluntaryExit> {
        let signed_voluntary_exits = self.signed_voluntary_exits.read();
        signed_voluntary_exits.values().cloned().collect()
    }

    pub fn clean_signed_voluntary_exits(&self, beacon_state: &BeaconState) {
        let mut signed_voluntary_exits = self.signed_voluntary_exits.write();
        let validator_indices = signed_voluntary_exits.keys().cloned().collect::<Vec<_>>();
        for validator_index in validator_indices {
            if beacon_state.validators[validator_index as usize].exit_epoch
                < beacon_state.finalized_checkpoint.epoch
            {
                signed_voluntary_exits.remove(&validator_index);
            }
        }
    }
}
