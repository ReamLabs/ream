use alloy_primitives::FixedBytes;
use ream_post_quantum_crypto::hashsig::signature::Signature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitList, VariableList, typenum::U4096};
use tree_hash_derive::TreeHash;

use crate::checkpoint::Checkpoint;

/// Attestation content describing the validator's observed chain view.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AttestationData {
    pub slot: u64,
    pub head: Checkpoint,
    pub target: Checkpoint,
    pub source: Checkpoint,
}

/// Validator specific attestation wrapping shared attestation data.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Attestation {
    pub validator_id: u64,
    pub data: AttestationData,
}

impl Attestation {
    /// Return the attested slot.
    pub fn slot(&self) -> u64 {
        self.data.slot
    }

    /// Return the attested head checkpoint.
    pub fn head(&self) -> Checkpoint {
        self.data.head
    }

    /// Return the attested target checkpoint.
    pub fn target(&self) -> Checkpoint {
        self.data.target
    }

    /// Return the attested source checkpoint.
    pub fn source(&self) -> Checkpoint {
        self.data.source
    }
}

/// Validator attestation bundled with its signature.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAttestation {
    pub message: Attestation,
    /// signature over attestaion message only as it would be aggregated later in attestation
    pub signature: Signature,
}

/// Aggregated attestation consisting of participation bits and message.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregatedAttestations {
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub aggregation_bits: BitList<U4096>,
    pub message: AttestationData,
}

/// Aggregated attestation bundled with aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAggregatedAttestation {
    pub message: AggregatedAttestations,
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub signature: VariableList<FixedBytes<4000>, U4096>,
}
