use alloy_primitives::B256;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use super::beacon_block_body::BeaconBlockBody;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBeaconBlock {
    pub message: BeaconBlock,
    pub signature: BLSSignature,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlock {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BeaconBlockBody,
}
