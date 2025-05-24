use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default, Eq, Hash)]
pub struct SecretKey {
    pub inner: B256,
}

impl SecretKey {
    pub fn to_bytes(&self) -> &[u8] {
        self.inner.as_slice()
    }
}
