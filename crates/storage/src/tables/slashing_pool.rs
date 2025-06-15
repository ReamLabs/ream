use std::{collections::HashMap, time::Instant};

use anyhow::{Error, Ok};
use parking_lot::RwLock;
use ream_consensus::proposer_slashing::ProposerSlashing;

#[derive(Debug)]
pub struct SlashingPool {
    proposer_slashings: RwLock<HashMap<u64, Vec<ProposerSlashing>>>,
    submission_time: RwLock<HashMap<u64, Instant>>,
}

impl Default for SlashingPool {
    fn default() -> Self {
        Self::new()
    }
}
impl SlashingPool {
    pub fn new() -> Self {
        Self {
            proposer_slashings: RwLock::new(HashMap::new()),
            submission_time: RwLock::new(HashMap::new()),
        }
    }

    /// Check if we have a slashing for the given proposer index
    pub fn has_slashing_for_proposer(&self, proposer_index: u64) -> bool {
        self.proposer_slashings.read().contains_key(&proposer_index)
    }

    /// Insert a proposer slashing into the pool.
    /// 
    pub fn insert_proposer_slashing(
        &self,
        proposer_slashing: ProposerSlashing,
    ) -> Result<(), Error> {
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

        let mut submission_time = self.submission_time.write();
        submission_time.insert(proposer_index, Instant::now());
        Ok(())
    }

    /// Get the proposer slashing
    pub fn get_all_proposer_slashings(&self) -> Vec<ProposerSlashing> {
        let proposer_slashings = self.proposer_slashings.read();
        proposer_slashings
            .values()
            .flat_map(|v| v.iter())
            .cloned()
            .collect()
    }

    /// Remove slashings that have been included in blocks
    pub fn prune_included_blocks(&self, included_indices: &[u64]) {
        let mut slashings = self.proposer_slashings.write();
        let mut times = self.submission_time.write();

        for index in included_indices {
            slashings.remove(index);
            times.remove(index);
        }
    }
}
