use std::hash::Hash;

use alloy_primitives::B256;
use ream_post_quantum_crypto::leansig::signature::Signature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
#[cfg(feature = "devnet5")]
use ssz_types::typenum::U524288;
use ssz_types::{BitList, VariableList, typenum::U4096};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::checkpoint::Checkpoint;

/// Key for signature storage, combining validator ID and attestation data root.
/// Used for both gossip_signatures and aggregated_payloads maps.
#[derive(
    Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, Encode, Decode, PartialOrd, Ord,
)]
pub struct SignatureKey {
    pub validator_id: u64,
    pub data_root: B256,
}

impl SignatureKey {
    pub fn new(validator_id: u64, attestation_data: &AttestationData) -> Self {
        Self {
            validator_id,
            data_root: attestation_data.tree_hash_root(),
        }
    }

    pub fn from_parts(validator_id: u64, data_root: B256) -> Self {
        Self {
            validator_id,
            data_root,
        }
    }
}

#[cfg(feature = "devnet5")]
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SingleMessageAggregate {
    pub participants: BitList<U4096>,
    pub proof: VariableList<u8, U524288>,
}

#[cfg(feature = "devnet5")]
impl SingleMessageAggregate {
    pub fn new(participants: BitList<U4096>, proof: VariableList<u8, U524288>) -> Self {
        Self {
            participants,
            proof,
        }
    }

    pub fn to_validator_indices(&self) -> Vec<u64> {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, bit)| *bit)
            .map(|(index, _)| index as u64)
            .collect()
    }
}

#[cfg(feature = "devnet5")]
#[derive(
    Debug, PartialEq, Eq, Clone, Hash, Default, Serialize, Deserialize, Encode, Decode, TreeHash,
)]
pub struct MultiMessageAggregate {
    pub proof: VariableList<u8, U524288>,
}

#[cfg(feature = "devnet5")]
impl MultiMessageAggregate {
    pub fn new(proof: VariableList<u8, U524288>) -> Self {
        Self { proof }
    }
}

#[cfg(feature = "devnet5")]
impl AsRef<[u8]> for MultiMessageAggregate {
    fn as_ref(&self) -> &[u8] {
        self.proof.as_ref()
    }
}

/// Attestation content describing the validator's observed chain view.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Hash)]
pub struct AttestationData {
    pub slot: u64,
    pub head: Checkpoint,
    pub target: Checkpoint,
    pub source: Checkpoint,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregatedAttestations {
    pub validator_id: u64,
    pub data: AttestationData,
}

impl AggregatedAttestation {
    /// Return the attested slot.
    pub fn slot(&self) -> u64 {
        self.message.slot
    }

    /// Return the attested head checkpoint.
    pub fn head(&self) -> Checkpoint {
        self.message.head
    }

    /// Return the attested target checkpoint.
    pub fn target(&self) -> Checkpoint {
        self.message.target
    }

    /// Return the attested source checkpoint.
    pub fn source(&self) -> Checkpoint {
        self.message.source
    }
}

/// Validator attestation bundled with its signature.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAttestation {
    pub validator_id: u64,
    pub message: AttestationData,
    /// signature over attestaion message only as it would be aggregated later in attestation
    pub signature: Signature,
}

/// Aggregated attestation consisting of participation bits and message.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregatedAttestation {
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub aggregation_bits: BitList<U4096>,
    pub message: AttestationData,
}

/// Aggregated attestation bundled with aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAggregatedAttestation {
    pub data: AttestationData,

    #[cfg(feature = "devnet5")]
    pub proof: SingleMessageAggregate,
}
