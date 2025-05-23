use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::signature::BLSSignature;

#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Serialize, Deserialize, Default)]
pub struct AggregateSignature {
    pub inner: BLSSignature,
}

impl AggregateSignature {
    pub fn to_signature(self) -> BLSSignature {
        self.inner
    }
}
