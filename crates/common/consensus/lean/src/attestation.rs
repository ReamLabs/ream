use alloy_primitives::FixedBytes;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitList, VariableList, typenum::U4096};
use tree_hash_derive::TreeHash;

use crate::vote::AttestationData;

/// Aggregated attestation consisting of participation bits and message.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#attestation)
/// for detailed protocol information.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggreagtedAttestation {
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub aggregation_bits: BitList<U4096>,
    pub message: AttestationData,
}

/// Aggregated attestation bundled with aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAggreagtedAttestation {
    pub message: AggreagtedAttestation,
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub signature: VariableList<FixedBytes<4000>, U4096>,
}
