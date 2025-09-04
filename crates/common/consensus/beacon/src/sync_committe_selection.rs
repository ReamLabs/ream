use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommitteeSelection {
    validatior_index: u64,
    slot: u64,
    subcommittee_index: u64,
    selection_proof: BLSSignature,
}
