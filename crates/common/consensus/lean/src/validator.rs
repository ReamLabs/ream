use ream_post_quantum_crypto::leansig::public_key::PublicKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

/// Represents a validator entry in the Lean chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    #[serde(alias = "attestationPubkey")]
    pub attestation_pubkey: PublicKey,
    #[serde(alias = "proposalPubkey")]
    pub proposal_pubkey: PublicKey,
    pub index: u64,
}

pub fn is_proposer(validator_index: u64, slot: u64, validator_count: u64) -> bool {
    slot % validator_count == validator_index
}

impl Validator {
    
    #[cfg(feature = "devnet4")]
    pub fn get_attestation_pubkey(&self) -> PublicKey {
        self.attestation_pubkey
    }

    #[cfg(feature = "devnet4")]
    pub fn get_proposal_pubkey(&self) -> PublicKey {
        self.proposal_pubkey
    }
}