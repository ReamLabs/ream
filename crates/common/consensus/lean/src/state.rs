use std::collections::HashMap;

use alloy_primitives::B256;
use ethereum_hashing::hash;
use serde::{Deserialize, Serialize};
use ssz_types::{
    VariableList,
    typenum::{
        U4096, // 2**12
    },
};


// TODO: Add back #[derive(Encode, Decode, TreeHash)]
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct LeanState {
    pub genesis_time: usize,
    pub stakers: VariableList<Staker, U4096>,
    pub num_validators: usize,

    pub latest_justified_hash: B256,
    pub latest_justified_slot: usize,
    pub latest_finalized_hash: B256,
    pub latest_finalized_slot: usize,

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
