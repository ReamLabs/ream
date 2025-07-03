use std::collections::HashMap;

use alloy_primitives::B256;
use parking_lot::RwLock;
use ream_consensus::{
    bls_to_execution_change::SignedBLSToExecutionChange, electra::beacon_state::BeaconState,
    voluntary_exit::SignedVoluntaryExit,
};
use tree_hash::TreeHash;

#[derive(Debug, Default)]
pub struct OperationPool {
    signed_voluntary_exits: RwLock<HashMap<u64, SignedVoluntaryExit>>,
    signed_bls_to_execution_changes: RwLock<HashMap<B256, SignedBLSToExecutionChange>>,
}

impl OperationPool {
    pub fn insert_signed_voluntary_exit(&self, signed_voluntary_exit: SignedVoluntaryExit) {
        self.signed_voluntary_exits.write().insert(
            signed_voluntary_exit.message.validator_index,
            signed_voluntary_exit,
        );
    }

    pub fn get_signed_voluntary_exits(&self) -> Vec<SignedVoluntaryExit> {
        self.signed_voluntary_exits
            .read()
            .values()
            .cloned()
            .collect()
    }

    pub fn clean_signed_voluntary_exits(&self, beacon_state: &BeaconState) {
        self.signed_voluntary_exits
            .write()
            .retain(|&validator_index, _| {
                beacon_state.validators[validator_index as usize].exit_epoch
                    >= beacon_state.finalized_checkpoint.epoch
            });
    }

    pub fn insert_signed_bls_to_execution_change(
        &self,
        signed_bls_to_execution_change: SignedBLSToExecutionChange,
    ) {
        self.signed_bls_to_execution_changes.write().insert(
            signed_bls_to_execution_change.tree_hash_root(),
            signed_bls_to_execution_change,
        );
    }

    pub fn get_signed_bls_to_execution_changes(&self) -> Vec<SignedBLSToExecutionChange> {
        self.signed_bls_to_execution_changes
            .read()
            .values()
            .cloned()
            .collect()
    }

    pub fn remove_signed_bls_to_execution_change(&self, root: B256) {
        self.signed_bls_to_execution_changes.write().remove(&root);
    }
}
