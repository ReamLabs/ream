use alloy_primitives::B256;
use ream_post_quantum_crypto::PQSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash_derive::TreeHash;

use crate::vote::Vote;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBlock {
    pub data: Block,
    pub signature: PQSignature,
}

#[derive(
    Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash,
)]
pub struct Block {
    pub slot: u64,
    // Diverged from Python implementation: Disallow `None` (uses `B256::ZERO` instead)
    pub parent: B256,
    pub votes: VariableList<Vote, U4096>,
    // Diverged from Python implementation: Disallow `None` (uses `B256::ZERO` instead)
    pub state_root: B256,
}
