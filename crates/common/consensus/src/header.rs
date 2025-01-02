use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;
use ream_common::primitives::{Root, Slot, ValidatorIndex};

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlockHeader {
    pub slot: Slot,
    pub proposer_index: ValidatorIndex,
    pub parent_root: Root,
    pub state_root: Root,
    pub body_root: Root,
}