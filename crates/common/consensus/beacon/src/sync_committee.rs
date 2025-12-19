use ream_bls::PublicKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U512};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommittee {
    #[serde(rename = "pubkeys")]
    pub public_keys: FixedVector<PublicKey, U512>,
    #[serde(rename = "aggregate_pubkey")]
    pub aggregate_public_key: PublicKey,
}
