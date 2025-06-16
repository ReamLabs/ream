use alloy_primitives::B256;
use ream_bls::BLSSignature;
use ream_consensus::electra::blinded_beacon_block_body::BlindedBeaconBlockBody;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlindedBeaconBlock {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BlindedBeaconBlockBody,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBlindedBeaconBlock {
    pub message: BlindedBeaconBlock,
    pub signature: BLSSignature,
}
