use std::slice::Iter;

use ream_consensus::fork::Fork;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkSchedule(pub [Fork; ForkSchedule::TOTAL]);

impl ForkSchedule {
    pub const TOTAL: usize = 6;

    pub const fn new(forks: [Fork; ForkSchedule::TOTAL]) -> Self {
        Self(forks)
    }

    pub fn iter(&self) -> Iter<'_, Fork> {
        self.0.iter()
    }

    pub fn scheduled(&self) -> impl Iterator<Item = &Fork> {
        self.iter()
            .filter(|fork| fork.epoch != Fork::UNSCHEDULED_EPOCH)
    }
}
