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
    pub expect_exception: Option<String>,
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
