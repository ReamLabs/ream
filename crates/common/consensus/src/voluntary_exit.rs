use ream_bls::BlsSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedVoluntaryExit {
    pub message: VoluntaryExit,
    pub signature: BlsSignature,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct VoluntaryExit {
    pub epoch: u64,
    pub validator_index: u64,
}
