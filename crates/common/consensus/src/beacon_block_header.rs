use alloy_primitives::B256;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::deneb::beacon_block::SignedBeaconBlock;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBeaconBlockHeader {
    pub message: BeaconBlockHeader,
    pub signature: BLSSignature,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body_root: B256,
}

pub fn compute_signed_block_header(signed_block: SignedBeaconBlock) -> SignedBeaconBlockHeader {
    let block = signed_block.message;
    let block_header = BeaconBlockHeader {
        slot: block.slot,
        proposer_index: block.proposer_index,
        parent_root: block.parent_root,
        state_root: block.state_root,
        body_root: block.body.tree_hash_root(),
    };
    SignedBeaconBlockHeader {
        message: block_header,
        signature: signed_block.signature,
    }
}
