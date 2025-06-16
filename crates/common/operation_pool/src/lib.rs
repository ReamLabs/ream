use std::{collections::HashMap, time::Instant};

use anyhow::{Error, Ok};
use parking_lot::RwLock;
use ream_consensus::{
    electra::beacon_state::BeaconState, proposer_slashing::ProposerSlashing,
    voluntary_exit::SignedVoluntaryExit,
};

#[derive(Debug, Default)]
pub struct OperationPool {
    signed_voluntary_exits: RwLock<HashMap<u64, SignedVoluntaryExit>>,
    proposer_slashings: RwLock<HashMap<u64, Vec<ProposerSlashing>>>,
    slashing_submission_time: RwLock<HashMap<u64, Instant>>,
}

impl OperationPool {
    pub fn new() -> Self {
        Self::default()
    }

    // Voluntary Exit operations
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

    // Proposer Slashing operations
    pub fn has_slashing_for_proposer(&self, proposer_index: u64) -> bool {
        self.proposer_slashings.read().contains_key(&proposer_index)
    }

    pub fn insert_proposer_slashing(
        &self,
        proposer_slashing: ProposerSlashing,
    ) -> anyhow::Result<(), Error> {
        let mut proposer_slashings = self.proposer_slashings.write();
        let proposer_index = proposer_slashing.signed_header_1.message.proposer_index;

        // Check if we already have a slashing for this validator
        if proposer_slashings.contains_key(&proposer_index) {
            return Err(Error::msg(
                "Proposer slashing already exists for this validator",
            ));
        }

        proposer_slashings
            .entry(proposer_index)
            .or_default()
            .push(proposer_slashing);

        let mut submission_time = self.slashing_submission_time.write();
        submission_time.insert(proposer_index, Instant::now());

        Ok(())
    }

    pub fn get_all_proposer_slashings(&self) -> Vec<ProposerSlashing> {
        let proposer_slashings = self.proposer_slashings.read();
        proposer_slashings
            .values()
            .flat_map(|v| v.iter())
            .cloned()
            .collect()
    }

    pub fn prune_included_slashings(&self, included_indices: &[u64]) {
        let mut slashings = self.proposer_slashings.write();
        let mut times = self.slashing_submission_time.write();

        for index in included_indices {
            slashings.remove(index);
            times.remove(index);
        }
    }
}
