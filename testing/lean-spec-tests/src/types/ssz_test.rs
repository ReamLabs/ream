//! SSZ test types with proper serde attributes for leanSpec JSON format.
//!
//! These intermediate types handle the camelCase JSON format from leanSpec fixtures
//! and provide conversions to the actual ream-consensus-lean types.

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
    typenum::{U4096, U1048576},
};

/// SSZ test case from leanSpec fixtures
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SSZTest {
    pub network: String,
    pub lean_env: String,
    pub type_name: String,
    pub value: serde_json::Value,
    pub serialized: String,
    pub root: B256,
}

// ============================================================================
// Intermediate types for JSON deserialization (camelCase)
// ============================================================================

/// Config with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigJSON {
    pub genesis_time: u64,
}

impl TryFrom<&ConfigJSON> for ReamConfig {
    type Error = anyhow::Error;

    fn try_from(config: &ConfigJSON) -> anyhow::Result<Self> {
        Ok(ReamConfig {
            genesis_time: config.genesis_time,
        })
    }
}

/// Checkpoint - already snake_case in JSON, but define for consistency
#[derive(Debug, Deserialize, Clone)]
pub struct CheckpointJSON {
    pub root: B256,
    pub slot: u64,
}

impl TryFrom<&CheckpointJSON> for ReamCheckpoint {
    type Error = anyhow::Error;

    fn try_from(cp: &CheckpointJSON) -> anyhow::Result<Self> {
        Ok(ReamCheckpoint {
            root: cp.root,
            slot: cp.slot,
        })
    }
}

/// AttestationData - already snake_case in JSON
#[derive(Debug, Deserialize, Clone)]
pub struct AttestationDataJSON {
    pub slot: u64,
    pub head: CheckpointJSON,
    pub target: CheckpointJSON,
    pub source: CheckpointJSON,
}

impl TryFrom<&AttestationDataJSON> for ReamAttestationData {
    type Error = anyhow::Error;

    fn try_from(data: &AttestationDataJSON) -> anyhow::Result<Self> {
        Ok(ReamAttestationData {
            slot: data.slot,
            head: (&data.head).try_into()?,
            target: (&data.target).try_into()?,
            source: (&data.source).try_into()?,
        })
    }
}

/// BlockHeader with camelCase fields
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

    fn try_from(header: &BlockHeaderJSON) -> anyhow::Result<Self> {
        Ok(ReamBlockHeader {
            slot: header.slot,
            proposer_index: header.proposer_index,
            parent_root: header.parent_root,
            state_root: header.state_root,
            body_root: header.body_root,
        })
    }
}

/// Validator with camelCase fields
#[derive(Debug, Deserialize, Clone)]
pub struct ValidatorJSON {
    pub pubkey: String,
    pub index: u64,
}

impl TryFrom<&ValidatorJSON> for ReamValidator {
    type Error = anyhow::Error;

    fn try_from(validator: &ValidatorJSON) -> anyhow::Result<Self> {
        let pubkey_hex = validator.pubkey.trim_start_matches("0x");
        let pubkey_bytes = alloy_primitives::hex::decode(pubkey_hex)
            .map_err(|err| anyhow!("Failed to decode validator pubkey hex: {err}"))?;

        if pubkey_bytes.len() != 52 {
            return Err(anyhow!(
                "Expected 52-byte pubkey, got {} bytes",
                pubkey_bytes.len()
            ));
        }

        Ok(ReamValidator {
            public_key: PublicKey::from(&pubkey_bytes[..]),
            index: validator.index,
        })
    }
}

/// Wrapper for data lists in JSON format
#[derive(Debug, Deserialize, Clone)]
pub struct DataListJSON<T> {
    pub data: Vec<T>,
}

/// AggregationBits wrapper
#[derive(Debug, Deserialize, Clone)]
pub struct AggregationBitsJSON {
    pub data: Vec<bool>,
}

/// AggregatedAttestation with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedAttestationJSON {
    pub aggregation_bits: AggregationBitsJSON,
    pub data: AttestationDataJSON,
}

impl TryFrom<&AggregatedAttestationJSON> for ReamAggregatedAttestation {
    type Error = anyhow::Error;

    fn try_from(att: &AggregatedAttestationJSON) -> anyhow::Result<Self> {
        let bool_data = &att.aggregation_bits.data;
        let mut aggregation_bits = BitList::<U4096>::with_capacity(bool_data.len())
            .map_err(|err| anyhow!("Failed to create BitList: {err:?}"))?;

        for (i, &bit) in bool_data.iter().enumerate() {
            aggregation_bits
                .set(i, bit)
                .map_err(|err| anyhow!("Failed to set bit at index {i}: {err:?}"))?;
        }

        Ok(ReamAggregatedAttestation {
            aggregation_bits,
            message: (&att.data).try_into()?,
        })
    }
}

/// Attestation (individual) with camelCase fields - maps to AggregatedAttestations in Rust
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AttestationJSON {
    pub validator_id: u64,
    pub data: AttestationDataJSON,
}

impl TryFrom<&AttestationJSON> for ReamAggregatedAttestations {
    type Error = anyhow::Error;

    fn try_from(att: &AttestationJSON) -> anyhow::Result<Self> {
        Ok(ReamAggregatedAttestations {
            validator_id: att.validator_id,
            data: (&att.data).try_into()?,
        })
    }
}

/// BlockBody with attestations in {data: [...]} format
#[derive(Debug, Deserialize, Clone)]
pub struct BlockBodyJSON {
    pub attestations: DataListJSON<AggregatedAttestationJSON>,
}

impl TryFrom<&BlockBodyJSON> for ReamBlockBody {
    type Error = anyhow::Error;

    fn try_from(body: &BlockBodyJSON) -> anyhow::Result<Self> {
        let mut attestations = Vec::new();
        for att in &body.attestations.data {
            attestations.push(ReamAggregatedAttestation::try_from(att)?);
        }

        Ok(ReamBlockBody {
            attestations: VariableList::try_from(attestations)
                .map_err(|err| anyhow!("Failed to create attestations VariableList: {err}"))?,
        })
    }
}

/// Block with camelCase fields
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

    fn try_from(block: &BlockJSON) -> anyhow::Result<Self> {
        Ok(ReamBlock {
            slot: block.slot,
            proposer_index: block.proposer_index,
            parent_root: block.parent_root,
            state_root: block.state_root,
            body: ReamBlockBody::try_from(&block.body)?,
        })
    }
}

/// State with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StateJSON {
    pub config: ConfigJSON,
    pub slot: u64,
    pub latest_block_header: BlockHeaderJSON,
    pub latest_justified: CheckpointJSON,
    pub latest_finalized: CheckpointJSON,
    pub historical_block_hashes: DataListJSON<B256>,
    pub justified_slots: DataListJSON<bool>,
    pub validators: DataListJSON<ValidatorJSON>,
    pub justifications_roots: DataListJSON<B256>,
    pub justifications_validators: DataListJSON<bool>,
}

impl TryFrom<&StateJSON> for ReamState {
    type Error = anyhow::Error;

    fn try_from(state: &StateJSON) -> anyhow::Result<Self> {
        use ssz_types::typenum::{U262144, U1073741824};

        // Convert validators
        let validators: Vec<ReamValidator> = state
            .validators
            .data
            .iter()
            .map(ReamValidator::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        // Convert justified_slots to BitList
        let justified_slots_len = state.justified_slots.data.len();
        let mut justified_slots = BitList::<U262144>::with_capacity(justified_slots_len)
            .map_err(|err| anyhow!("Failed to create justified_slots BitList: {err:?}"))?;
        for (i, &bit) in state.justified_slots.data.iter().enumerate() {
            justified_slots
                .set(i, bit)
                .map_err(|err| anyhow!("Failed to set justified_slots bit at {i}: {err:?}"))?;
        }

        // Convert justifications_validators to BitList
        let justifications_len = state.justifications_validators.data.len();
        let mut justifications_validators =
            BitList::<U1073741824>::with_capacity(justifications_len).map_err(|err| {
                anyhow!("Failed to create justifications_validators BitList: {err:?}")
            })?;
        for (i, &bit) in state.justifications_validators.data.iter().enumerate() {
            justifications_validators.set(i, bit).map_err(|err| {
                anyhow!("Failed to set justifications_validators bit at {i}: {err:?}")
            })?;
        }

        Ok(ReamState {
            config: (&state.config).try_into()?,
            slot: state.slot,
            latest_block_header: (&state.latest_block_header).try_into()?,
            latest_justified: (&state.latest_justified).try_into()?,
            latest_finalized: (&state.latest_finalized).try_into()?,
            historical_block_hashes: VariableList::try_from(
                state.historical_block_hashes.data.clone(),
            )
            .map_err(|err| anyhow!("Failed to create historical_block_hashes: {err}"))?,
            justified_slots,
            validators: VariableList::try_from(validators)
                .map_err(|err| anyhow!("Failed to create validators: {err}"))?,
            justifications_roots: VariableList::try_from(state.justifications_roots.data.clone())
                .map_err(|err| {
                anyhow!("Failed to create justifications_roots: {err}")
            })?,
            justifications_validators,
        })
    }
}

// ============================================================================
// Signature-related types
// ============================================================================

/// SignedAttestation with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedAttestationJSON {
    pub validator_id: u64,
    pub message: AttestationDataJSON,
    pub signature: String,
}

impl TryFrom<&SignedAttestationJSON> for ReamSignedAttestation {
    type Error = anyhow::Error;

    fn try_from(att: &SignedAttestationJSON) -> anyhow::Result<Self> {
        let sig_hex = att.signature.trim_start_matches("0x");
        let sig_bytes = alloy_primitives::hex::decode(sig_hex)
            .map_err(|err| anyhow!("Failed to decode signature hex: {err}"))?;

        let signature = Signature::from_ssz_bytes(&sig_bytes)
            .map_err(|err| anyhow!("Failed to decode signature from SSZ: {err:?}"))?;

        Ok(ReamSignedAttestation {
            validator_id: att.validator_id,
            message: (&att.message).try_into()?,
            signature,
        })
    }
}

/// Wrapper for proof_data in JSON format (nested { data: "0x..." })
#[derive(Debug, Deserialize, Clone)]
pub struct ProofDataJSON {
    pub data: String,
}

/// AggregatedSignatureProof with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedSignatureProofJSON {
    pub participants: AggregationBitsJSON,
    pub proof_data: ProofDataJSON,
}

impl TryFrom<&AggregatedSignatureProofJSON> for ReamAggregatedSignatureProof {
    type Error = anyhow::Error;

    fn try_from(proof: &AggregatedSignatureProofJSON) -> anyhow::Result<Self> {
        let bool_data = &proof.participants.data;
        let mut participants = BitList::<U4096>::with_capacity(bool_data.len())
            .map_err(|err| anyhow!("Failed to create BitList: {err:?}"))?;

        for (i, &bit) in bool_data.iter().enumerate() {
            participants
                .set(i, bit)
                .map_err(|err| anyhow!("Failed to set bit at index {i}: {err:?}"))?;
        }

        let proof_hex = proof.proof_data.data.trim_start_matches("0x");
        let proof_bytes = alloy_primitives::hex::decode(proof_hex)
            .map_err(|err| anyhow!("Failed to decode proof_data hex: {err}"))?;

        Ok(ReamAggregatedSignatureProof {
            participants,
            proof_data: VariableList::<u8, U1048576>::try_from(proof_bytes)
                .map_err(|err| anyhow!("Failed to create proof_data: {err}"))?,
        })
    }
}

/// BlockSignatures with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockSignaturesJSON {
    pub attestation_signatures: DataListJSON<AggregatedSignatureProofJSON>,
    pub proposer_signature: String,
}

impl TryFrom<&BlockSignaturesJSON> for ReamBlockSignatures {
    type Error = anyhow::Error;

    fn try_from(sigs: &BlockSignaturesJSON) -> anyhow::Result<Self> {
        let mut attestation_signatures = Vec::new();
        for proof in &sigs.attestation_signatures.data {
            attestation_signatures.push(ReamAggregatedSignatureProof::try_from(proof)?);
        }

        let sig_hex = sigs.proposer_signature.trim_start_matches("0x");
        let sig_bytes = alloy_primitives::hex::decode(sig_hex)
            .map_err(|err| anyhow!("Failed to decode proposer_signature hex: {err}"))?;

        let proposer_signature = Signature::from_ssz_bytes(&sig_bytes)
            .map_err(|err| anyhow!("Failed to decode proposer_signature from SSZ: {err:?}"))?;

        Ok(ReamBlockSignatures {
            attestation_signatures: VariableList::try_from(attestation_signatures)
                .map_err(|err| anyhow!("Failed to create attestation_signatures: {err}"))?,
            proposer_signature,
        })
    }
}

/// BlockWithAttestation with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockWithAttestationJSON {
    pub block: BlockJSON,
    pub proposer_attestation: AttestationJSON,
}

impl TryFrom<&BlockWithAttestationJSON> for ReamBlockWithAttestation {
    type Error = anyhow::Error;

    fn try_from(bwa: &BlockWithAttestationJSON) -> anyhow::Result<Self> {
        Ok(ReamBlockWithAttestation {
            block: (&bwa.block).try_into()?,
            proposer_attestation: (&bwa.proposer_attestation).try_into()?,
        })
    }
}

/// SignedBlockWithAttestation with camelCase fields
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedBlockWithAttestationJSON {
    pub message: BlockWithAttestationJSON,
    pub signature: BlockSignaturesJSON,
}

impl TryFrom<&SignedBlockWithAttestationJSON> for ReamSignedBlockWithAttestation {
    type Error = anyhow::Error;

    fn try_from(sbwa: &SignedBlockWithAttestationJSON) -> anyhow::Result<Self> {
        Ok(ReamSignedBlockWithAttestation {
            message: (&sbwa.message).try_into()?,
            signature: (&sbwa.signature).try_into()?,
        })
    }
}
