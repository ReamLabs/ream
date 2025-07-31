use std::collections::HashMap;

use ethereum_hashing::hash;
use serde::{Deserialize, Serialize};
use ssz_types::{
    VariableList,
    typenum::{
        U4096, // 2**12
    },
};

use crate::{
    Hash,
    staker::Staker,
};

// TODO: Add back #[derive(Encode, Decode, TreeHash)]
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct LeanState {
    pub genesis_time: usize,
    pub stakers: VariableList<Staker, U4096>,
    pub num_validators: usize,

    pub latest_justified_hash: Hash,
    pub latest_justified_slot: usize,
    pub latest_finalized_hash: Hash,
    pub latest_finalized_slot: usize,

    pub historical_block_hashes: VariableList<Option<Hash>, U4096>,
    pub justified_slots: VariableList<bool, U4096>,

    pub justifications: HashMap<Hash, Vec<bool>>,
}

impl LeanState {
    pub fn compute_hash(&self) -> Hash {
        let serialized = serde_json::to_string(self).unwrap();
        Hash::from_slice(&hash(serialized.as_bytes()))
    }
}
