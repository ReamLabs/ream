use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::VariableList;
use std::collections::HashMap;
use tree_hash_derive::TreeHash;

use crate::{
    staker::Staker
    Hash,
};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct LeanState {
    pub genesis_time: u64,
    pub stakers: VariableList<Staker, ssz_types::typenum::U1000000>,

    pub latest_justified_hash: Hash,
    pub latest_justified_slot: usize,
    pub latest_finalized_hash: Hash,
    pub latest_finalized_slot: usize,
    pub historical_block_hashes: Vec<Option<Hash>>,
    pub justified_slots: Vec<bool>,
    pub justifications: HashMap<Hash, Vec<bool>>,
}

impl State {
    pub fn compute_hash(&self) -> Hash {
        let serialized = serde_json::to_string(self).unwrap();
        Hash::from_slice(&hash(serialized.as_bytes()))
    }
}
