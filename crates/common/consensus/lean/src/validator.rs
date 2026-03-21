use ream_post_quantum_crypto::leansig::public_key::PublicKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

/// Represents a validator entry in the Lean chain.
#[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    #[serde(rename = "pubkey", alias = "attestationPubkey")]
    pub public_key: PublicKey,
    pub index: u64,
}

/// Represents a validator entry in the Lean chain.
#[cfg(feature = "devnet4")]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    #[serde(alias = "attestationPubkey", alias = "pubkey")]
    pub attestation_pubkey: PublicKey,
    #[serde(alias = "proposalPubkey")]
    pub proposal_pubkey: PublicKey,
    pub index: u64,
}

impl Validator {
    #[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
    pub fn from_public_key(public_key: PublicKey, index: u64) -> Self {
        Self { public_key, index }
    }

    #[cfg(feature = "devnet4")]
    pub fn from_public_key(public_key: PublicKey, index: u64) -> Self {
        Self {
            attestation_pubkey: public_key,
            proposal_pubkey: public_key,
            index,
        }
    }

    #[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
    pub fn from_pubkeys(
        attestation_pubkey: PublicKey,
        proposal_pubkey: PublicKey,
        index: u64,
    ) -> Self {
        debug_assert_eq!(attestation_pubkey, proposal_pubkey);
        Self {
            public_key: attestation_pubkey,
            index,
        }
    }

    #[cfg(feature = "devnet4")]
    pub fn from_pubkeys(
        attestation_pubkey: PublicKey,
        proposal_pubkey: PublicKey,
        index: u64,
    ) -> Self {
        Self {
            attestation_pubkey,
            proposal_pubkey,
            index,
        }
    }

    #[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
    pub fn attestation_pubkey(&self) -> PublicKey {
        self.public_key
    }

    #[cfg(feature = "devnet4")]
    pub fn attestation_pubkey(&self) -> PublicKey {
        self.attestation_pubkey
    }

    #[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
    pub fn proposal_pubkey(&self) -> PublicKey {
        self.public_key
    }

    #[cfg(feature = "devnet4")]
    pub fn proposal_pubkey(&self) -> PublicKey {
        self.proposal_pubkey
    }

    #[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
    pub fn set_pubkeys(&mut self, attestation_pubkey: PublicKey, proposal_pubkey: PublicKey) {
        debug_assert_eq!(attestation_pubkey, proposal_pubkey);
        self.public_key = attestation_pubkey;
    }

    #[cfg(feature = "devnet4")]
    pub fn set_pubkeys(&mut self, attestation_pubkey: PublicKey, proposal_pubkey: PublicKey) {
        self.attestation_pubkey = attestation_pubkey;
        self.proposal_pubkey = proposal_pubkey;
    }
}

pub fn is_proposer(validator_index: u64, slot: u64, validator_count: u64) -> bool {
    slot % validator_count == validator_index
}
