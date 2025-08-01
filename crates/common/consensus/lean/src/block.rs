use alloy_primitives::B256;
use ethereum_hashing::hash;
use ream_pqc::PQSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    VariableList,
    typenum::{
        U16777216, // 2**24
    },
};

use crate::vote::Vote;

// TODO: Add back #[derive(TreeHash)]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SignedBlock {
    pub message: Block,
    pub signature: PQSignature,
}

// TODO: Add back #[derive(TreeHash)]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, Default)]
pub struct Block {
    pub slot: usize,
    pub proposer_index: usize,
    pub parent: Option<B256>,
    pub votes: VariableList<Vote, U16777216>,
    pub state_root: Option<B256>,
}

impl Block {
    pub fn compute_hash(&self) -> B256 {
        let serialized = serde_json::to_string(self).unwrap();
        B256::from_slice(&hash(serialized.as_bytes()))
    }
}
