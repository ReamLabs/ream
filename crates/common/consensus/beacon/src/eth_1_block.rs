use alloy_primitives::B256;
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
    pub deposit_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_count: u64,
    pub block_hash: B256,
}

impl Eth1Block {
    pub fn is_candidate_block(&self, period_start: u64) -> bool {
        self.timestamp + SECONDS_PER_ETH1_BLOCK * ETH1_FOLLOW_DISTANCE <= period_start
            && self.timestamp + SECONDS_PER_ETH1_BLOCK * ETH1_FOLLOW_DISTANCE * 2 >= period_start
    }

    pub fn eth1_data(&self) -> Eth1Data {
        Eth1Data {
            deposit_root: self.deposit_root,
            deposit_count: self.deposit_count,
            block_hash: self.block_hash,
        }
    }
}
