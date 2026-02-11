//! Intermediate JSON types for leanSpec SSZ test fixtures.
//!
//! These types handle camelCase JSON and convert to ream-consensus-lean types.
//!
//! These intermediate conversions are needed because the test vectors define the
//! expected deserialized keys & values as JSON and in camelCase while Rust and our
//! codebase uses snake_case.

use alloy_primitives::B256;
use anyhow::anyhow;
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation as ReamAggregatedAttestation,
        AggregatedAttestations as ReamAggregatedAttestations,
        AggregatedSignatureProof as ReamAggregatedSignatureProof,
        AttestationData as ReamAttestationData, SignedAttestation as ReamSignedAttestation,
    },
    block::{
        Block as ReamBlock, BlockBody as ReamBlockBody, BlockHeader as ReamBlockHeader,
        BlockSignatures as ReamBlockSignatures, BlockWithAttestation as ReamBlockWithAttestation,
        SignedBlockWithAttestation as ReamSignedBlockWithAttestation,
    },
    checkpoint::Checkpoint as ReamCheckpoint,
    config::Config as ReamConfig,
    state::LeanState as ReamState,
    validator::Validator as ReamValidator,
};
use ream_post_quantum_crypto::leansig::{public_key::PublicKey, signature::Signature};
use serde::Deserialize;
use ssz::Decode;
use ssz_types::{
    BitList, VariableList,
    typenum::{U262144, U1048576, U1073741824},
};

// ============================================================================
// Helpers
// ============================================================================

fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    alloy_primitives::hex::decode(hex.trim_start_matches("0x"))
        .map_err(|error| anyhow!("hex decode failed: {error}"))
}

fn decode_signature(hex: &str) -> anyhow::Result<Signature> {
    Signature::from_ssz_bytes(&decode_hex(hex)?)
        .map_err(|error| anyhow!("signature decode failed: {error:?}"))
}

fn bools_to_bitlist<N: ssz_types::typenum::Unsigned>(bools: &[bool]) -> anyhow::Result<BitList<N>> {
    let mut bits = BitList::<N>::with_capacity(bools.len())
        .map_err(|error| anyhow!("BitList creation failed: {error:?}"))?;
    for (index, &bit) in bools.iter().enumerate() {
        bits.set(index, bit)
            .map_err(|error| anyhow!("BitList set failed: {error:?}"))?;
    }
    Ok(bits)
}

// ============================================================================
// Test case structure
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SSZTest {
    pub network: String,
    pub lean_env: String,
    pub type_name: String,
    pub value: serde_json::Value,
    pub serialized: String,
}

// ============================================================================
// Macros
// ============================================================================

/// Creates a passthrough JSON wrapper that deserializes directly to the inner type.
macro_rules! passthrough_conversion {
    ($name:ident, $inner:ty) => {
        #[derive(Debug, Deserialize, Clone)]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl TryFrom<&$name> for $inner {
            type Error = anyhow::Error;
            fn try_from(value: &$name) -> anyhow::Result<Self> {
                Ok(value.0.clone())
            }
        }
    };
}

/// Creates a TryFrom impl where all fields are copied directly.
macro_rules! simple_conversion {
    ($json:ident => $target:ty { $($field:ident),+ }) => {
        impl TryFrom<&$json> for $target {
            type Error = anyhow::Error;
            fn try_from(value: &$json) -> anyhow::Result<Self> {
                Ok(Self {
                    $($field: value.$field),+
                })
            }
        }
    };
}

/// Creates a TryFrom impl where all fields are converted via try_into().
macro_rules! nested_conversion {
    ($json:ident => $target:ty { $($field:ident),+ }) => {
        impl TryFrom<&$json> for $target {
            type Error = anyhow::Error;
            fn try_from(value: &$json) -> anyhow::Result<Self> {
                Ok(Self {
                    $($field: (&value.$field).try_into()?),+
                })
            }
        }
    };
}

/// Creates a TryFrom impl with custom conversion expression per field.
macro_rules! custom_conversion {
    ($json:ident as $val:ident => $target:ty { $($field:ident: $conv:expr),+ $(,)? }) => {
        impl TryFrom<&$json> for $target {
            type Error = anyhow::Error;
            #[allow(clippy::redundant_closure_call)]
            fn try_from($val: &$json) -> anyhow::Result<Self> {
                Ok(Self {
                    $($field: $conv),+
                })
            }
        }
    };
}

// ============================================================================
// Common JSON wrapper types
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
pub struct DataListJSON<T> {
    pub data: Vec<T>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AggregationBitsJSON {
    pub data: Vec<bool>,
}

passthrough_conversion!(CheckpointJSON, ReamCheckpoint);
passthrough_conversion!(AttestationDataJSON, ReamAttestationData);

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigJSON {
    pub genesis_time: u64,
}

simple_conversion!(ConfigJSON => ReamConfig { genesis_time });

// ============================================================================
// BlockHeader
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeaderJSON {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body_root: B256,
}

simple_conversion!(BlockHeaderJSON => ReamBlockHeader { slot, proposer_index, parent_root, state_root, body_root });

// ============================================================================
// Validator
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
pub struct ValidatorJSON {
    pub pubkey: String,
    pub index: u64,
}

custom_conversion!(ValidatorJSON as value => ReamValidator {
    public_key: {
        let bytes = decode_hex(&value.pubkey)?;
        if bytes.len() != 52 {
            return Err(anyhow!("Expected 52-byte pubkey, got {}", bytes.len()));
        }
        PublicKey::from(&bytes[..])
    },
    index: value.index,
});

// ============================================================================
// Attestations
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedAttestationJSON {
    pub aggregation_bits: AggregationBitsJSON,
    pub data: ReamAttestationData,
}

custom_conversion!(AggregatedAttestationJSON as value => ReamAggregatedAttestation {
    aggregation_bits: bools_to_bitlist(&value.aggregation_bits.data)?,
    message: value.data.clone(),
});

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AttestationJSON {
    pub validator_id: u64,
    pub data: ReamAttestationData,
}

custom_conversion!(AttestationJSON as value => ReamAggregatedAttestations {
    validator_id: value.validator_id,
    data: value.data.clone(),
});

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedAttestationJSON {
    pub validator_id: u64,
    pub message: ReamAttestationData,
    pub signature: String,
}

custom_conversion!(SignedAttestationJSON as value => ReamSignedAttestation {
    validator_id: value.validator_id,
    message: value.message.clone(),
    signature: decode_signature(&value.signature)?,
});

// ============================================================================
// Block
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
pub struct BlockBodyJSON {
    pub attestations: DataListJSON<AggregatedAttestationJSON>,
}

custom_conversion!(BlockBodyJSON as value => ReamBlockBody {
    attestations: VariableList::try_from(
        value.attestations.data.iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?
    ).map_err(|error| anyhow!("{error}"))?,
});

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockJSON {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BlockBodyJSON,
}

custom_conversion!(BlockJSON as value => ReamBlock {
    slot: value.slot,
    proposer_index: value.proposer_index,
    parent_root: value.parent_root,
    state_root: value.state_root,
    body: (&value.body).try_into()?,
});

// ============================================================================
// State
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StateJSON {
    pub config: ConfigJSON,
    pub slot: u64,
    pub latest_block_header: BlockHeaderJSON,
    pub latest_justified: ReamCheckpoint,
    pub latest_finalized: ReamCheckpoint,
    pub historical_block_hashes: DataListJSON<B256>,
    pub justified_slots: DataListJSON<bool>,
    pub validators: DataListJSON<ValidatorJSON>,
    pub justifications_roots: DataListJSON<B256>,
    pub justifications_validators: DataListJSON<bool>,
}

custom_conversion!(StateJSON as value => ReamState {
    config: (&value.config).try_into()?,
    slot: value.slot,
    latest_block_header: (&value.latest_block_header).try_into()?,
    latest_justified: value.latest_justified,
    latest_finalized: value.latest_finalized,
    historical_block_hashes: VariableList::try_from(value.historical_block_hashes.data.clone())
        .map_err(|error| anyhow!("{error}"))?,
    justified_slots: bools_to_bitlist::<U262144>(&value.justified_slots.data)?,
    validators: VariableList::try_from(
        value.validators.data.iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?
    ).map_err(|error| anyhow!("{error}"))?,
    justifications_roots: VariableList::try_from(value.justifications_roots.data.clone())
        .map_err(|error| anyhow!("{error}"))?,
    justifications_validators: bools_to_bitlist::<U1073741824>(&value.justifications_validators.data)?,
});

// ============================================================================
// Signature-related types
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
pub struct ProofDataJSON {
    pub data: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedSignatureProofJSON {
    pub participants: AggregationBitsJSON,
    pub proof_data: ProofDataJSON,
}

custom_conversion!(AggregatedSignatureProofJSON as value => ReamAggregatedSignatureProof {
    participants: bools_to_bitlist(&value.participants.data)?,
    proof_data: VariableList::<u8, U1048576>::try_from(decode_hex(&value.proof_data.data)?)
        .map_err(|error| anyhow!("{error}"))?,
});

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockSignaturesJSON {
    pub attestation_signatures: DataListJSON<AggregatedSignatureProofJSON>,
    pub proposer_signature: String,
}

custom_conversion!(BlockSignaturesJSON as value => ReamBlockSignatures {
    attestation_signatures: VariableList::try_from(
        value.attestation_signatures.data.iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?
    ).map_err(|error| anyhow!("{error}"))?,
    proposer_signature: decode_signature(&value.proposer_signature)?,
});

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockWithAttestationJSON {
    pub block: BlockJSON,
    pub proposer_attestation: AttestationJSON,
}

nested_conversion!(BlockWithAttestationJSON => ReamBlockWithAttestation { block, proposer_attestation });

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedBlockWithAttestationJSON {
    pub message: BlockWithAttestationJSON,
    pub signature: BlockSignaturesJSON,
}

nested_conversion!(SignedBlockWithAttestationJSON => ReamSignedBlockWithAttestation { message, signature });
