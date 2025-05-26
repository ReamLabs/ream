use alloy_primitives::B256;

/// Common hashing function for Merkle trees.
pub fn hash_concat(h1: &[u8], h2: &[u8]) -> B256 {
    ethereum_hashing::hash32_concat(h1, h2).into()
}
