use alloy_primitives::B256;
use serde::Deserialize;

use crate::types::{Block, State};

/// State transition test case
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateTransitionTest {
    pub network: String,
    pub pre: State,
    pub blocks: Vec<Block>,
    pub post: Option<StateExpectation>,
    /// devnet4 field name for expected failure
    pub expect_exception: Option<String>,
    /// devnet5 field name for expected failure
    pub rejection_reason: Option<String>,
}

impl StateTransitionTest {
    pub fn expects_failure(&self) -> bool {
        self.expect_exception.is_some() || self.rejection_reason.is_some()
    }
}

/// State expectations for state transition tests
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateExpectation {
    pub slot: Option<u64>,
    pub latest_block_header_slot: Option<u64>,
    pub latest_block_header_state_root: Option<B256>,
    pub historical_block_hashes_count: Option<usize>,
}
