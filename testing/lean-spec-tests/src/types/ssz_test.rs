//! Intermediate JSON types for leanSpec SSZ test fixtures.
//!
//! These types handle camelCase JSON and convert to ream-consensus-lean types.
//!
//! These intermediate conversions are needed because the test vectors define the
//! expected deserialized keys & values as JSON and in camelCase while Rust and our
//! codebase uses snake_case.

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
#[cfg(feature = "devnet3")]
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof, AttestationData,
        SignedAttestation,
    },
    block::{
        Block, BlockBody, BlockHeader, BlockSignatures, BlockWithAttestation,
        SignedBlockWithAttestation,
    },
    checkpoint::Checkpoint,
    config::Config,
    state::LeanState,
    validator::Validator,
};
#[cfg(feature = "devnet4")]
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof, AttestationData,
        SignedAttestation,
    },
    block::{Block, BlockBody, BlockHeader, BlockSignatures, SignedBlock},
    checkpoint::Checkpoint,
    config::Config,
    state::LeanState,
    validator::Validator,
};
use ream_post_quantum_crypto::leansig::{public_key::PublicKey, signature::Signature};
use serde::Deserialize;
use ssz_types::{
    BitList, VariableList,
    typenum::{U1024, U262144, U1073741824},
};

// ============================================================================
// Helpers
// ============================================================================

fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    alloy_primitives::hex::decode(hex.trim_start_matches("0x"))
        .map_err(|err| anyhow!("hex decode failed: {err}"))
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

fn decode_signature(hex: &str) -> anyhow::Result<Signature> {
    let bytes = decode_hex(hex)?;
    ensure!(
        bytes.len() == 3112,
        "Expected 3112-byte signature, got {} bytes",
        bytes.len()
    );
    Ok(Signature::from(&bytes[..]))
}

fn decode_public_key(hex: &str) -> anyhow::Result<PublicKey> {
    let bytes = decode_hex(hex)?;
    ensure!(
        bytes.len() == 52,
        "Expected 52-byte pubkey, got {}",
        bytes.len()
    );
    Ok(PublicKey::from(&bytes[..]))
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
pub struct ProofDataJSON {
    pub data: String,
}

// ============================================================================
// Consensus types
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(transparent)]
pub struct CheckpointJSON(pub Checkpoint);

impl TryFrom<&CheckpointJSON> for Checkpoint {
    type Error = anyhow::Error;

    fn try_from(value: &CheckpointJSON) -> anyhow::Result<Self> {
        Ok(value.0)
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(transparent)]
pub struct AttestationDataJSON(pub AttestationData);

impl TryFrom<&AttestationDataJSON> for AttestationData {
    type Error = anyhow::Error;

    fn try_from(value: &AttestationDataJSON) -> anyhow::Result<Self> {
        Ok(value.0.clone())
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigJSON {
    pub genesis_time: u64,
}

impl TryFrom<&ConfigJSON> for Config {
    type Error = anyhow::Error;

    fn try_from(value: &ConfigJSON) -> anyhow::Result<Self> {
        Ok(Self {
            genesis_time: value.genesis_time,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeaderJSON {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body_root: B256,
}

impl TryFrom<&BlockHeaderJSON> for BlockHeader {
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

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorJSON {
    pub pubkey: Option<String>,
    pub attestation_pubkey: Option<String>,
    pub proposal_pubkey: Option<String>,
    pub index: u64,
}

impl TryFrom<&ValidatorJSON> for Validator {
    type Error = anyhow::Error;

    fn try_from(value: &ValidatorJSON) -> anyhow::Result<Self> {
        let attestation_pubkey = value
            .attestation_pubkey
            .as_deref()
            .or(value.pubkey.as_deref())
            .ok_or_else(|| anyhow!("validator is missing attestation pubkey"))?;
        let proposal_pubkey = value
            .proposal_pubkey
            .as_deref()
            .or(value.pubkey.as_deref())
            .unwrap_or(attestation_pubkey);

        Ok(Self {
            attestation_pubkey: decode_public_key(attestation_pubkey)?,
            proposal_pubkey: decode_public_key(proposal_pubkey)?,
            index: value.index,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedAttestationJSON {
    pub aggregation_bits: DataListJSON<bool>,
    pub data: AttestationData,
}

impl TryFrom<&AggregatedAttestationJSON> for AggregatedAttestation {
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
    pub data: AttestationData,
}

impl TryFrom<&AttestationJSON> for AggregatedAttestations {
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
pub struct BlockBodyJSON {
    pub attestations: DataListJSON<AggregatedAttestationJSON>,
}

impl TryFrom<&BlockBodyJSON> for BlockBody {
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
            .map_err(|err| anyhow!("Failed to convert attestations: {err}"))?,
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

impl TryFrom<&BlockJSON> for Block {
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

#[cfg(feature = "devnet3")]
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockWithAttestationJSON {
    pub block: BlockJSON,
    pub proposer_attestation: AttestationJSON,
}

#[cfg(feature = "devnet3")]
impl TryFrom<&BlockWithAttestationJSON> for BlockWithAttestation {
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
pub struct StateJSON {
    pub config: ConfigJSON,
    pub slot: u64,
    pub latest_block_header: BlockHeaderJSON,
    pub latest_justified: Checkpoint,
    pub latest_finalized: Checkpoint,
    pub historical_block_hashes: DataListJSON<B256>,
    pub justified_slots: DataListJSON<bool>,
    pub validators: DataListJSON<ValidatorJSON>,
    pub justifications_roots: DataListJSON<B256>,
    pub justifications_validators: DataListJSON<bool>,
}

impl TryFrom<&StateJSON> for LeanState {
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
            .map_err(|err| anyhow!("Failed to convert historical_block_hashes: {err}"))?,
            justified_slots: bools_to_bitlist::<U262144>(&value.justified_slots.data)?,
            validators: VariableList::try_from(
                value
                    .validators
                    .data
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|err| anyhow!("Failed to convert validators: {err}"))?,
            justifications_roots: VariableList::try_from(value.justifications_roots.data.clone())
                .map_err(|err| {
                anyhow!("Failed to convert justifications_roots: {err}")
            })?,
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
#[serde(rename_all = "camelCase")]
pub struct SignedAttestationJSON {
    pub validator_id: u64,
    pub data: AttestationData,
    pub signature: String,
}

impl TryFrom<&SignedAttestationJSON> for SignedAttestation {
    type Error = anyhow::Error;

    fn try_from(value: &SignedAttestationJSON) -> anyhow::Result<Self> {
        Ok(Self {
            validator_id: value.validator_id,
            message: value.data.clone(),
            signature: decode_signature(&value.signature)?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockSignaturesJSON {
    pub attestation_signatures: DataListJSON<AggregatedSignatureProofJSON>,
    pub proposer_signature: String,
}

impl TryFrom<&BlockSignaturesJSON> for BlockSignatures {
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
            .map_err(|err| anyhow!("Failed to convert attestation_signatures: {err}"))?,
            proposer_signature: decode_signature(&value.proposer_signature)?,
        })
    }
}

#[cfg(feature = "devnet3")]
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedBlockWithAttestationJSON {
    pub message: BlockWithAttestationJSON,
    pub signature: BlockSignaturesJSON,
}

#[cfg(feature = "devnet3")]
impl TryFrom<&SignedBlockWithAttestationJSON> for SignedBlockWithAttestation {
    type Error = anyhow::Error;

    fn try_from(value: &SignedBlockWithAttestationJSON) -> anyhow::Result<Self> {
        Ok(Self {
            message: (&value.message).try_into()?,
            signature: (&value.signature).try_into()?,
        })
    }
}

#[cfg(feature = "devnet4")]
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SignedBlockJSON {
    pub message: BlockJSON,
    pub signature: BlockSignaturesJSON,
}

#[cfg(feature = "devnet4")]
impl TryFrom<&SignedBlockJSON> for SignedBlock {
    type Error = anyhow::Error;

    fn try_from(value: &SignedBlockJSON) -> anyhow::Result<Self> {
        Ok(Self {
            message: (&value.message).try_into()?,
            signature: (&value.signature).try_into()?,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedSignatureProofJSON {
    pub participants: DataListJSON<bool>,
    pub proof_data: ProofDataJSON,
}

impl TryFrom<&AggregatedSignatureProofJSON> for AggregatedSignatureProof {
    type Error = anyhow::Error;

    fn try_from(value: &AggregatedSignatureProofJSON) -> anyhow::Result<Self> {
        Ok(Self {
            participants: bools_to_bitlist(&value.participants.data)?,
            proof_data: VariableList::try_from(decode_hex(&value.proof_data.data)?)
                .map_err(|err| anyhow!("Failed to convert proof_data: {err}"))?,
            #[cfg(feature = "devnet4")]
            bytecode_point: None,
        })
    }
}

// ============================================================================
// Networking types
// ============================================================================

/// Local Status type matching ream-req-resp's Status SSZ layout.
/// Defined locally to avoid pulling in heavy networking dependencies.
#[derive(Debug, Deserialize, Clone, ssz_derive::Encode)]
pub struct StatusJSON {
    pub finalized: Checkpoint,
    pub head: Checkpoint,
}

impl TryFrom<&StatusJSON> for StatusJSON {
    type Error = anyhow::Error;

    fn try_from(value: &StatusJSON) -> anyhow::Result<Self> {
        Ok(value.clone())
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlocksByRootRequestJSON {
    pub roots: DataListJSON<B256>,
}

impl TryFrom<&BlocksByRootRequestJSON> for BlocksByRootRequestSSZ {
    type Error = anyhow::Error;

    fn try_from(value: &BlocksByRootRequestJSON) -> anyhow::Result<Self> {
        Ok(Self {
            roots: VariableList::try_from(value.roots.data.clone())
                .map_err(|err| anyhow!("Failed to convert roots: {err}"))?,
        })
    }
}

/// Newtype for SSZ encoding of BlocksByRootRequest
#[derive(ssz_derive::Encode)]
pub struct BlocksByRootRequestSSZ {
    pub roots: VariableList<B256, U1024>,
}

// ============================================================================
// XMSS types
// ============================================================================

/// JSON representation of a Signature as a hex string.
#[derive(Debug, Deserialize, Clone)]
#[serde(transparent)]
pub struct SignatureJSON(pub String);

impl TryFrom<&SignatureJSON> for Signature {
    type Error = anyhow::Error;

    fn try_from(value: &SignatureJSON) -> anyhow::Result<Self> {
        decode_signature(&value.0)
    }
}

/// JSON representation of a PublicKey with structured root/parameter arrays.
/// Each array element is a u32 KoalaBear field element.
#[derive(Debug, Deserialize, Clone)]
pub struct PublicKeyJSON {
    pub root: DataListJSON<u32>,
    pub parameter: DataListJSON<u32>,
}

impl TryFrom<&PublicKeyJSON> for PublicKey {
    type Error = anyhow::Error;

    fn try_from(value: &PublicKeyJSON) -> anyhow::Result<Self> {
        ensure!(
            value.root.data.len() == 8,
            "Expected 8 root elements, got {}",
            value.root.data.len()
        );
        ensure!(
            value.parameter.data.len() == 5,
            "Expected 5 parameter elements, got {}",
            value.parameter.data.len()
        );

        let bytes: Vec<u8> = value
            .root
            .data
            .iter()
            .chain(value.parameter.data.iter())
            .flat_map(|e| e.to_le_bytes())
            .collect();

        Ok(PublicKey::from(&bytes[..]))
    }
}
