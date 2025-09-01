use std::collections::HashMap;

use alloy_primitives::{Address, B256, map::HashSet};
use parking_lot::RwLock;
use ream_consensus_beacon::{
    attester_slashing::AttesterSlashing, bls_to_execution_change::SignedBLSToExecutionChange,
    electra::beacon_state::BeaconState, proposer_slashing::ProposerSlashing,
    voluntary_exit::SignedVoluntaryExit,
};
use tree_hash::TreeHash;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposerPreparation {
    pub fee_recipient: Address,
    pub submission_epoch: u64,
}

#[derive(Debug, Default)]
pub struct OperationPool {
    signed_voluntary_exits: RwLock<HashMap<u64, SignedVoluntaryExit>>,
    signed_bls_to_execution_changes: RwLock<HashMap<B256, SignedBLSToExecutionChange>>,
    proposer_preparations: RwLock<HashMap<u64, ProposerPreparation>>,
    attester_slashings: RwLock<HashSet<AttesterSlashing>>,
    proposer_slashings: RwLock<HashSet<ProposerSlashing>>,
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

    pub fn insert_proposer_preparation(
        &self,
        validator_index: u64,
        fee_recipient: Address,
        submission_epoch: u64,
    ) {
        self.proposer_preparations.write().insert(
            validator_index,
            ProposerPreparation {
                fee_recipient,
                submission_epoch,
            },
        );
    }

    pub fn get_proposer_preparation(&self, validator_index: u64) -> Option<Address> {
        self.proposer_preparations
            .read()
            .get(&validator_index)
            .map(|preparation| preparation.fee_recipient)
    }

    pub fn get_all_proposer_preparations(&self) -> HashMap<u64, Address> {
        self.proposer_preparations
            .read()
            .iter()
            .map(|(&index, preparation)| (index, preparation.fee_recipient))
            .collect()
    }

    pub fn clean_proposer_preparations(&self, current_epoch: u64) {
        self.proposer_preparations.write().retain(|_, preparation| {
            // Keep preparations that are still valid
            // They persist through the epoch of submission and for 2 more epochs after that
            current_epoch <= preparation.submission_epoch + 2
        });
    }

    pub fn insert_attester_slashing(&self, slashing: AttesterSlashing) {
        self.attester_slashings.write().insert(slashing);
    }

    pub fn get_all_attester_slashings(&self) -> Vec<AttesterSlashing> {
        self.attester_slashings.read().iter().cloned().collect()
    }

    pub fn get_all_proposer_slahsings(&self) -> Vec<ProposerSlashing> {
        self.proposer_slashings.read().iter().cloned().collect()
    }

    pub fn insert_proposer_slashing(&self, slashing: ProposerSlashing) {
        self.proposer_slashings.write().insert(slashing);
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

        operation_pool.insert_proposer_preparation(1, fee_recipient1, 100);
        assert_eq!(
            operation_pool.get_proposer_preparation(1),
            Some(fee_recipient1)
        );

        operation_pool.insert_proposer_preparation(2, fee_recipient2, 100);
        let all_preparations = operation_pool.get_all_proposer_preparations();
        assert_eq!(all_preparations.len(), 2);
        assert_eq!(all_preparations.get(&1), Some(&fee_recipient1));
        assert_eq!(all_preparations.get(&2), Some(&fee_recipient2));

        operation_pool.insert_proposer_preparation(1, fee_recipient2, 101);
        assert_eq!(
            operation_pool.get_proposer_preparation(1),
            Some(fee_recipient2)
        );
    }

    #[test]
    fn test_proposer_preparation_expiration() {
        let operation_pool = OperationPool::default();
        let fee_recipient1 = Address::from([0x11; 20]);
        let fee_recipient2 = Address::from([0x22; 20]);
        let fee_recipient3 = Address::from([0x33; 20]);

        // Insert preparations at different epochs
        operation_pool.insert_proposer_preparation(1, fee_recipient1, 100);
        operation_pool.insert_proposer_preparation(2, fee_recipient2, 101);
        operation_pool.insert_proposer_preparation(3, fee_recipient3, 102);

        // All should be present initially
        assert_eq!(operation_pool.get_all_proposer_preparations().len(), 3);

        // Clean at epoch 102 - all should still be valid
        operation_pool.clean_proposer_preparations(102);
        assert_eq!(operation_pool.get_all_proposer_preparations().len(), 3);

        // Clean at epoch 103 - validator 1 (epoch 100) should be expired
        operation_pool.clean_proposer_preparations(103);
        let remaining = operation_pool.get_all_proposer_preparations();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining.get(&1), None);
        assert_eq!(remaining.get(&2), Some(&fee_recipient2));
        assert_eq!(remaining.get(&3), Some(&fee_recipient3));

        // Clean at epoch 104 - validators 1 and 2 should be expired
        operation_pool.clean_proposer_preparations(104);
        let remaining = operation_pool.get_all_proposer_preparations();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining.get(&3), Some(&fee_recipient3));

        // Clean at epoch 105 - all should be expired
        operation_pool.clean_proposer_preparations(105);
        assert_eq!(operation_pool.get_all_proposer_preparations().len(), 0);
    }

    #[test]
    fn test_proposer_preparation_edge_cases() {
        let operation_pool = OperationPool::default();
        let fee_recipient = Address::from([0x11; 20]);

        // Test exact boundary - submission at epoch 100 is valid through epoch 102
        operation_pool.insert_proposer_preparation(1, fee_recipient, 100);

        // Should be valid at epoch 102
        operation_pool.clean_proposer_preparations(102);
        assert_eq!(
            operation_pool.get_proposer_preparation(1),
            Some(fee_recipient)
        );

        // Should be expired at epoch 103
        operation_pool.clean_proposer_preparations(103);
        assert_eq!(operation_pool.get_proposer_preparation(1), None);
    }
}
