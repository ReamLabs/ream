use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

use crate::{
    constants::{ETH1_FOLLOW_DISTANCE, SECONDS_PER_ETH1_BLOCK},
    eth_1_data::Eth1Data,
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Eth1Block {
    pub number: u64,
    pub timestamp: u64,
    pub eth1_data: Eth1Data,
}

impl Eth1Block {
    pub fn is_candidate_block(&self, period_start: u64) -> bool {
        self.timestamp + SECONDS_PER_ETH1_BLOCK * ETH1_FOLLOW_DISTANCE <= period_start
            && self.timestamp + SECONDS_PER_ETH1_BLOCK * ETH1_FOLLOW_DISTANCE * 2 >= period_start
    }
}
