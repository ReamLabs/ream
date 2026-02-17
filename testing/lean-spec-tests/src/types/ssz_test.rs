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
    typenum::{U1048576, U1073741824, U262144},
    BitList, VariableList,
};

// ============================================================================
// Helpers
// ============================================================================

fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    alloy_primitives::hex::decode(hex.trim_start_matches("0x"))
        .map_err(|err| anyhow!("hex decode failed: {err}"))
}

fn decode_signature(hex: &str) -> anyhow::Result<Signature> {
    Signature::from_ssz_bytes(&decode_hex(hex)?)
        .map_err(|err| anyhow!("signature decode failed: {err:?}"))
}

fn bools_to_bitlist<N: ssz_types::typenum::Unsigned>(bools: &[bool]) -> anyhow::Result<BitList<N>> {
    let mut bits = BitList::<N>::with_capacity(bools.len())
        .map_err(|err| anyhow!("BitList creation failed: {err:?}"))?;
    for (index, &bit) in bools.iter().enumerate() {
        bits.set(index, bit)
            .map_err(|err| anyhow!("BitList set failed: {err:?}"))?;
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

#[derive(Debug, Deserialize, Clone)]
#[serde(transparent)]
pub struct CheckpointJSON(pub ReamCheckpoint);

impl TryFrom<&CheckpointJSON> for ReamCheckpoint {
    type Error = anyhow::Error;

    fn try_from(value: &CheckpointJSON) -> anyhow::Result<Self> {
        Ok(value.0.clone())
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(transparent)]
pub struct AttestationDataJSON(pub ReamAttestationData);

impl TryFrom<&AttestationDataJSON> for ReamAttestationData {
    type Error = anyhow::Error;

    fn try_from(value: &AttestationDataJSON) -> anyhow::Result<Self> {
        Ok(value.0.clone())
    }
}

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigJSON {
    pub genesis_time: u64,
}

impl TryFrom<&ConfigJSON> for ReamConfig {
    type Error = anyhow::Error;

    fn try_from(value: &ConfigJSON) -> anyhow::Result<Self> {
        Ok(Self {
            genesis_time: value.genesis_time,
        })
    }
}

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

impl TryFrom<&BlockHeaderJSON> for ReamBlockHeader {
    type Error = anyhow::Error;

    fn try_from(value: &BlockHeaderJSON) -> anyhow::Result<Self> {
        Ok(Self {
            slot: value.slot,
            proposer_index: value.proposer_index,
            parent_root: value.parent_root,
            state_root: value.state_root,
            body_root: value.body_root,
        })
    }
}

// ============================================================================
// Validator
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
pub struct ValidatorJSON {
    pub pubkey: String,
    pub index: u64,
}

impl TryFrom<&ValidatorJSON> for ReamValidator {
    type Error = anyhow::Error;

    fn try_from(value: &ValidatorJSON) -> anyhow::Result<Self> {
        let bytes = decode_hex(&value.pubkey)?;
        if bytes.len() != 52 {
            return Err(anyhow!("Expected 52-byte pubkey, got {}", bytes.len()));
        }

        Ok(Self {
            public_key: PublicKey::from(&bytes[..]),
            index: value.index,
        })
    }
}

// ============================================================================
// Attestations
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedAttestationJSON {
    pub aggregation_bits: AggregationBitsJSON,
    pub data: ReamAttestationData,
}

impl TryFrom<&AggregatedAttestationJSON> for ReamAggregatedAttestation {
    type Error = anyhow::Error;

    fn try_from(value: &AggregatedAttestationJSON) -> anyhow::Result<Self> {
        Ok(Self {
            aggregation_bits: bools_to_bitlist(&value.aggregation_bits.data)?,
            message: value.data.clone(),
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AttestationJSON {
    pub validator_id: u64,
    pub data: ReamAttestationData,
}

impl TryFrom<&AttestationJSON> for ReamAggregatedAttestations {
    type Error = anyhow::Error;

    fn try_from(value: &AttestationJSON) -> anyhow::Result<Self> {
        Ok(Self {
            validator_id: value.validator_id,
            data: value.data.clone(),
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedAttestationJSON {
    pub validator_id: u64,
    pub message: ReamAttestationData,
    pub signature: String,
}

impl TryFrom<&SignedAttestationJSON> for ReamSignedAttestation {
    type Error = anyhow::Error;

    fn try_from(value: &SignedAttestationJSON) -> anyhow::Result<Self> {
        Ok(Self {
            validator_id: value.validator_id,
            message: value.message.clone(),
            signature: decode_signature(&value.signature)?,
        })
    }
}

// ============================================================================
// Block
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
pub struct BlockBodyJSON {
    pub attestations: DataListJSON<AggregatedAttestationJSON>,
}

impl TryFrom<&BlockBodyJSON> for ReamBlockBody {
    type Error = anyhow::Error;

    fn try_from(value: &BlockBodyJSON) -> anyhow::Result<Self> {
        Ok(Self {
            attestations: VariableList::try_from(
                value
                    .attestations
                    .data
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|err| anyhow!("{err}"))?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockJSON {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BlockBodyJSON,
}

impl TryFrom<&BlockJSON> for ReamBlock {
    type Error = anyhow::Error;

    fn try_from(value: &BlockJSON) -> anyhow::Result<Self> {
        Ok(Self {
            slot: value.slot,
            proposer_index: value.proposer_index,
            parent_root: value.parent_root,
            state_root: value.state_root,
            body: (&value.body).try_into()?,
        })
    }
}

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

impl TryFrom<&StateJSON> for ReamState {
    type Error = anyhow::Error;

    fn try_from(value: &StateJSON) -> anyhow::Result<Self> {
        Ok(Self {
            config: (&value.config).try_into()?,
            slot: value.slot,
            latest_block_header: (&value.latest_block_header).try_into()?,
            latest_justified: value.latest_justified,
            latest_finalized: value.latest_finalized,
            historical_block_hashes: VariableList::try_from(
                value.historical_block_hashes.data.clone(),
            )
            .map_err(|err| anyhow!("{err}"))?,
            justified_slots: bools_to_bitlist::<U262144>(&value.justified_slots.data)?,
            validators: VariableList::try_from(
                value
                    .validators
                    .data
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|err| anyhow!("{err}"))?,
            justifications_roots: VariableList::try_from(value.justifications_roots.data.clone())
                .map_err(|err| anyhow!("{err}"))?,
            justifications_validators: bools_to_bitlist::<U1073741824>(
                &value.justifications_validators.data,
            )?,
        })
    }
}

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

impl TryFrom<&AggregatedSignatureProofJSON> for ReamAggregatedSignatureProof {
    type Error = anyhow::Error;

    fn try_from(value: &AggregatedSignatureProofJSON) -> anyhow::Result<Self> {
        Ok(Self {
            participants: bools_to_bitlist(&value.participants.data)?,
            proof_data: VariableList::<u8, U1048576>::try_from(decode_hex(&value.proof_data.data)?)
                .map_err(|err| anyhow!("{err}"))?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockSignaturesJSON {
    pub attestation_signatures: DataListJSON<AggregatedSignatureProofJSON>,
    pub proposer_signature: String,
}

impl TryFrom<&BlockSignaturesJSON> for ReamBlockSignatures {
    type Error = anyhow::Error;

    fn try_from(value: &BlockSignaturesJSON) -> anyhow::Result<Self> {
        Ok(Self {
            attestation_signatures: VariableList::try_from(
                value
                    .attestation_signatures
                    .data
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|err| anyhow!("{err}"))?,
            proposer_signature: decode_signature(&value.proposer_signature)?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockWithAttestationJSON {
    pub block: BlockJSON,
    pub proposer_attestation: AttestationJSON,
}

impl TryFrom<&BlockWithAttestationJSON> for ReamBlockWithAttestation {
    type Error = anyhow::Error;

    fn try_from(value: &BlockWithAttestationJSON) -> anyhow::Result<Self> {
        Ok(Self {
            block: (&value.block).try_into()?,
            proposer_attestation: (&value.proposer_attestation).try_into()?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedBlockWithAttestationJSON {
    pub message: BlockWithAttestationJSON,
    pub signature: BlockSignaturesJSON,
}

impl TryFrom<&SignedBlockWithAttestationJSON> for ReamSignedBlockWithAttestation {
    type Error = anyhow::Error;

    fn try_from(value: &SignedBlockWithAttestationJSON) -> anyhow::Result<Self> {
        Ok(Self {
            message: (&value.message).try_into()?,
            signature: (&value.signature).try_into()?,
        })
    }
}
