use alloy_primitives::B256;
use ethereum_hashing::hash;
use serde::{Deserialize, Serialize};
use ssz_types::{typenum::U4096, VariableList};
use std::collections::HashMap;

use crate::config::Config;

// TODO: Add back #[derive(Encode, Decode, TreeHash)]
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct LeanState {
    pub config: Config,

    pub latest_justified_hash: B256,
    pub latest_justified_slot: u64,
    pub latest_finalized_hash: B256,
    pub latest_finalized_slot: u64,

    pub historical_block_hashes: VariableList<Option<B256>, U4096>,
    pub justified_slots: VariableList<bool, U4096>,

    pub justifications: HashMap<B256, Vec<bool>>,
}

impl LeanState {
    pub fn compute_hash(&self) -> B256 {
        let serialized = serde_json::to_string(self).unwrap();
        B256::from_slice(&hash(serialized.as_bytes()))
    }
}
