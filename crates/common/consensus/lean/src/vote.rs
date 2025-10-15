
use alloy_primitives::FixedBytes;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::checkpoint::Checkpoint;

/// Attestation content describing the validator's observed chain view.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#vote)
/// for detailed protocol information.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AttestationData {
    pub slot: u64,
    pub head: Checkpoint,
    pub target: Checkpoint,
    pub source: Checkpoint,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ValidatorAttestation {
    pub validator_id: u64,
    pub data: AttestationData,
}

impl ValidatorAttestation {
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
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#signedvote)
/// for detailed protocol information.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedValidatorAttestation {
    pub message: ValidatorAttestation,
    // signature over vote message only as it would be aggregated later in attestation
    pub signature: FixedBytes<4000>,
}
