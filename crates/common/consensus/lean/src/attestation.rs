use alloy_primitives::FixedBytes;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitList, VariableList, typenum::U4096};
use tree_hash_derive::TreeHash;

use crate::checkpoint::Checkpoint;

/// Attestation content describing the validator's observed chain view.
///
/// TODO: Add link from spec once finalized.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AttestationData {
    pub slot: u64,
    pub head: Checkpoint,
    pub target: Checkpoint,
    pub source: Checkpoint,
}

/// Validator specific attestation wrapping shared attestation data.
///
/// TODO: Add link from spec once finalized.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Attestation {
    pub validator_id: u64,
    pub data: AttestationData,
}

impl Attestation {
    // Return the attested slot.
    pub fn slot(&self) -> u64 {
        self.data.slot
    }

    // Return the attested head checkpoint.
    pub fn head(&self) -> Checkpoint {
        self.data.head
    }

    // Return the attested target checkpoint.
    pub fn target(&self) -> Checkpoint {
        self.data.target
    }

    // Return the attested source checkpoint.
    pub fn source(&self) -> Checkpoint {
        self.data.source
    }
}

/// Validator attestation bundled with its signature.
///
/// TODO: Add link from spec once finalized.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAttestation {
    pub message: Attestation,
    // signature over attestaion message only as it would be aggregated later in attestation
    pub signature: FixedBytes<4000>,
}

/// Aggregated attestation consisting of participation bits and message.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#attestation)
/// for detailed protocol information.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregatedAttestation {
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub aggregation_bits: BitList<U4096>,
    pub message: AttestationData,
}

/// Aggregated attestation bundled with aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAggregatedAttestation {
    pub message: AggregatedAttestation,
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub signature: VariableList<FixedBytes<4000>, U4096>,
}
