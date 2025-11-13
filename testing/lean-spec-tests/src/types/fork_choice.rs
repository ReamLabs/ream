use alloy_primitives::B256;
use anyhow::{Result, anyhow};
use ream_consensus_lean::{
    block::BlockHeader as ReamBlockHeader, checkpoint::Checkpoint as ReamCheckpoint,
    config::Config, state::LeanState, validator::Validator,
};
use serde::Deserialize;
use ssz_types::VariableList;

use crate::types::{Attestation, Block, Checkpoint, State};

/// Fork choice test case
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkChoiceTest {
    pub network: String,
    pub anchor_state: State,
    pub anchor_block: Block,
    pub steps: Vec<ForkChoiceStep>,
}

/// Fork choice step - can be tick, block, attestation, or checks
#[derive(Debug, Deserialize)]
#[serde(tag = "stepType", rename_all = "lowercase")]
pub enum ForkChoiceStep {
    Tick {
        #[serde(default)]
        valid: Option<bool>,
        time: u64,
    },
    Block {
        valid: bool,
        checks: Option<StoreChecks>,
        block: BlockWithProposerAttestation,
    },
    Attestation {
        valid: bool,
        checks: Option<StoreChecks>,
        attestation: Attestation,
    },
    Checks {
        checks: StoreChecks,
    },
}

/// Block wrapper containing the block and its proposer attestation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockWithProposerAttestation {
    pub block: Block,
    pub proposer_attestation: Attestation,
}

/// Store checks for fork choice validation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreChecks {
    pub head_slot: Option<u64>,
    pub head_root: Option<B256>,
    pub time: Option<u64>,
    pub justified_checkpoint: Option<Checkpoint>,
    pub finalized_checkpoint: Option<Checkpoint>,
    pub proposer_boost_root: Option<B256>,
    #[serde(default)]
    pub attestation_checks: Vec<AttestationCheck>,
}

/// Attestation check for validating attestation state
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttestationCheck {
    pub validator: u64,
    pub attestation_slot: u64,
    pub target_slot: Option<u64>,
    pub location: String,
}

// TryFrom implementation for converting State to LeanState

impl TryFrom<State> for LeanState {
    type Error = anyhow::Error;

    fn try_from(state: State) -> Result<Self> {
        let validators: Vec<Validator> = state
            .validators
            .data
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<_>>>()?;

        // Convert historical_block_hashes
        let historical_block_hashes =
            VariableList::try_from(state.historical_block_hashes.data.clone()).map_err(|err| {
                anyhow!("Failed to create historical_block_hashes VariableList: {err}")
            })?;

        // Convert justified_slots - this is a BitList in ream but Vec<u64> in leanSpec
        let justified_slots = {
            if state.justified_slots.data.is_empty() {
                // Empty BitList if no justified slots
                ssz_types::BitList::with_capacity(0)
                    .map_err(|err| anyhow!("Failed to create empty BitList: {err:?}"))?
            } else {
                let max_slot = state
                    .justified_slots
                    .data
                    .iter()
                    .max()
                    .copied()
                    .expect("Failed to get max slot");
                let mut bitlist = ssz_types::BitList::with_capacity(max_slot as usize + 1)
                    .map_err(|err| {
                        anyhow!(
                            "Failed to create BitList with capacity {}: {err:?}",
                            max_slot + 1
                        )
                    })?;

                for &slot in &state.justified_slots.data {
                    bitlist
                        .set(slot as usize, true)
                        .map_err(|err| anyhow!("Failed to set bit at index {slot}: {err:?}"))?;
                }
                bitlist
            }
        };

        let justifications_roots = VariableList::try_from(state.justifications_roots.data.clone())
            .map_err(|err| anyhow!("Failed to create justifications_roots VariableList: {err}"))?;

        let justifications_validators = {
            let validator_count = validators.len();
            let total_bits = state.justifications_validators.data.len() * validator_count;

            let mut bitlist = ssz_types::BitList::with_capacity(total_bits).map_err(|err| {
                anyhow!("Failed to create BitList for justifications_validators: {err:?}")
            })?;

            for (root_index, validator_list) in
                state.justifications_validators.data.iter().enumerate()
            {
                for &validator_index in validator_list {
                    let flat_index = root_index * validator_count + validator_index as usize;
                    bitlist.set(flat_index, true).map_err(|err| {
                        anyhow!("Failed to set bit at flat index {flat_index}: {err:?}")
                    })?;
                }
            }
            bitlist
        };

        Ok(LeanState {
            config: Config::from(&state.config),
            slot: state.slot,
            latest_block_header: ReamBlockHeader::from(&state.latest_block_header),
            latest_justified: ReamCheckpoint::from(&state.latest_justified),
            latest_finalized: ReamCheckpoint::from(&state.latest_finalized),
            historical_block_hashes,
            justified_slots,
            validators: VariableList::try_from(validators)
                .map_err(|err| anyhow!("Failed to create validators VariableList: {err}"))?,
            justifications_roots,
            justifications_validators,
        })
    }
}
