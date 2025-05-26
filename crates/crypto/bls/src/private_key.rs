use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default, Eq, Hash)]
pub struct PrivateKey {
    pub inner: B256,
}
