use ream_post_quantum_crypto::hashsig::public_key::PublicKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

/// Represents a validator entry in the Lean chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    #[serde(rename = "pubkey")]
    pub public_key: PublicKey,
}

impl Validator {
    pub fn generate_default_validators(number_of_validators: usize) -> Vec<Validator> {
        (0..number_of_validators)
            .map(|_| Validator {
                public_key: PublicKey::from(&[0_u8; 52][..]),
            })
            .collect()
    }
}

pub fn is_proposer(validator_index: u64, slot: u64, validator_count: u64) -> bool {
    slot % validator_count == validator_index
}
