use std::collections::HashMap;

use alloy_primitives::{Address, B256};
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
    proposer_preparations: RwLock<HashMap<u64, Address>>,
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

    pub fn insert_proposer_preparation(&self, validator_index: u64, fee_recipient: Address) {
        self.proposer_preparations
            .write()
            .insert(validator_index, fee_recipient);
    }

    pub fn get_proposer_preparation(&self, validator_index: u64) -> Option<Address> {
        self.proposer_preparations
            .read()
            .get(&validator_index)
            .copied()
    }

    pub fn get_all_proposer_preparations(&self) -> HashMap<u64, Address> {
        self.proposer_preparations.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposer_preparation_operations() {
        let operation_pool = OperationPool::default();
        let fee_recipient1 = Address::from([0x11; 20]);
        let fee_recipient2 = Address::from([0x22; 20]);

        assert_eq!(operation_pool.get_proposer_preparation(1), None);

        operation_pool.insert_proposer_preparation(1, fee_recipient1);
        assert_eq!(
            operation_pool.get_proposer_preparation(1),
            Some(fee_recipient1)
        );

        operation_pool.insert_proposer_preparation(2, fee_recipient2);
        let all_preparations = operation_pool.get_all_proposer_preparations();
        assert_eq!(all_preparations.len(), 2);
        assert_eq!(all_preparations.get(&1), Some(&fee_recipient1));
        assert_eq!(all_preparations.get(&2), Some(&fee_recipient2));

        operation_pool.insert_proposer_preparation(1, fee_recipient2);
        assert_eq!(
            operation_pool.get_proposer_preparation(1),
            Some(fee_recipient2)
        );
    }
}
