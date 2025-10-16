use ream_post_quantum_crypto::hashsig::public_key::PublicKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

/// Represents a validator entry in the Lean chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    #[serde(rename = "pubkey")]
    public_key: PublicKey,
}
