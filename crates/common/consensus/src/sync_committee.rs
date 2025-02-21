use ream_bls::PubKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{typenum::U512, FixedVector};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommittee {
    pub pubkeys: FixedVector<PubKey, U512>,
    pub aggregate_pubkey: PubKey,
}
