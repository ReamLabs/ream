use alloy_primitives::B256;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::beacon_block_body::BeaconBlockBody;
use crate::beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBeaconBlock {
    pub message: BeaconBlock,
    pub signature: BLSSignature,
}

impl SignedBeaconBlock {
    pub fn compute_signed_block_header(&self) -> SignedBeaconBlockHeader {
        let block_header = BeaconBlockHeader {
            slot: self.message.slot,
            proposer_index: self.message.proposer_index,
            parent_root: self.message.parent_root,
            state_root: self.message.state_root,
            body_root: self.message.body.tree_hash_root(),
        };
        SignedBeaconBlockHeader {
            message: block_header,
            signature: self.signature.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlock {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BeaconBlockBody,
}
