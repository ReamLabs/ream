pub mod fork_choice;
pub mod state_transition;

use std::collections::HashMap;

use alloy_primitives::{B256, hex};
use anyhow::{anyhow, bail};
use ream_consensus_lean::{
    attestation::{Attestation as ReamAttestation, AttestationData},
    block::{Block as ReamBlock, BlockBody as ReamBlockBody, BlockHeader as ReamBlockHeader},
    checkpoint::Checkpoint as ReamCheckpoint,
    config::Config as ReamConfig,
    validator::Validator as ReamValidator,
};
use ream_post_quantum_crypto::leansig::public_key::PublicKey;
use serde::Deserialize;
use ssz_types::VariableList;

/// A leanSpec test fixture file contains a map of test IDs to test cases
pub type TestFixture<T> = HashMap<String, T>;

/// Common fields in all test fixtures
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseTestCase {
    pub network: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// State config for test fixtures
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StateConfig {
    pub genesis_time: u64,
}

/// Block header for test fixtures
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body_root: B256,
}

/// Checkpoint for test fixtures
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub root: B256,
    pub slot: u64,
}

/// Validator
#[derive(Debug, Deserialize, Clone)]
pub struct Validator {
    pub pubkey: String,
    pub index: u64,
}

/// Block
/// Note: JSON uses both camelCase (anchorBlock) and snake_case (steps.block) formats
#[derive(Debug, Deserialize)]
pub struct Block {
    pub slot: u64,
    #[serde(alias = "proposerIndex")]
    pub proposer_index: u64,
    #[serde(alias = "parentRoot")]
    pub parent_root: B256,
    #[serde(alias = "stateRoot")]
    pub state_root: B256,
    pub body: BlockBody,
}

/// Block body
#[derive(Debug, Deserialize)]
pub struct BlockBody {
    pub attestations: DataList<Attestation>,
}

/// Attestation
#[derive(Debug, Deserialize)]
pub struct Attestation {
    #[serde(alias = "validatorId")]
    pub validator_id: u64,
    pub data: AttestationData,
}

/// Generic data list wrapper
#[derive(Debug, Deserialize)]
pub struct DataList<T> {
    pub data: Vec<T>,
}

/// Consensus state
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub config: StateConfig,
    pub slot: u64,
    pub latest_block_header: BlockHeader,
    pub latest_justified: Checkpoint,
    pub latest_finalized: Checkpoint,
    pub historical_block_hashes: DataList<B256>,
    pub justified_slots: DataList<u64>,
    pub validators: DataList<Validator>,
    pub justifications_roots: DataList<B256>,
    pub justifications_validators: DataList<Vec<u64>>,
}

impl<T> DataList<T> {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// From/TryFrom implementations for converting JSON types to ream consensus types

impl From<&StateConfig> for ReamConfig {
    fn from(config: &StateConfig) -> Self {
        ream_consensus_lean::config::Config {
            genesis_time: config.genesis_time,
        }
    }
}

impl From<&BlockHeader> for ReamBlockHeader {
    fn from(header: &BlockHeader) -> Self {
        ream_consensus_lean::block::BlockHeader {
            slot: header.slot,
            proposer_index: header.proposer_index,
            parent_root: header.parent_root,
            state_root: header.state_root,
            body_root: header.body_root,
        }
    }
}

impl From<&Checkpoint> for ReamCheckpoint {
    fn from(checkpoint: &Checkpoint) -> Self {
        ream_consensus_lean::checkpoint::Checkpoint {
            root: checkpoint.root,
            slot: checkpoint.slot,
        }
    }
}

impl TryFrom<&Validator> for ReamValidator {
    type Error = anyhow::Error;

    fn try_from(validator: &Validator) -> anyhow::Result<Self> {
        // Parse hex pubkey string
        let pubkey_hex = validator.pubkey.trim_start_matches("0x");
        let pubkey_bytes = hex::decode(pubkey_hex)
            .map_err(|err| anyhow!("Failed to decode validator pubkey hex: {err}"))?;

        // LeanSpec uses 52-byte XMSS public keys - verify the size
        if pubkey_bytes.len() != 52 {
            bail!("Expected 52-byte pubkey, got {} bytes", pubkey_bytes.len());
        }

        Ok(ReamValidator {
            public_key: PublicKey::from(&pubkey_bytes[..]),
            index: validator.index,
        })
    }
}

impl From<&Attestation> for ReamAttestation {
    fn from(attestation: &Attestation) -> Self {
        ReamAttestation {
            validator_id: attestation.validator_id,
            data: attestation.data.clone(),
        }
    }
}

impl TryFrom<&Block> for ReamBlock {
    type Error = anyhow::Error;

    fn try_from(block: &Block) -> anyhow::Result<Self> {
        let attestations: Vec<ReamAttestation> = block
            .body
            .attestations
            .data
            .iter()
            .map(ReamAttestation::from)
            .collect();

        Ok(ReamBlock {
            slot: block.slot,
            proposer_index: block.proposer_index,
            parent_root: block.parent_root,
            state_root: block.state_root,
            body: ReamBlockBody {
                attestations: VariableList::try_from(attestations)
                    .map_err(|err| anyhow!("Failed to create attestations VariableList: {err}"))?,
            },
        })
    }
}
