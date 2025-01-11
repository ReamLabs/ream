use std::{cmp::max, sync::Arc};

use alloy_primitives::B256;
use anyhow::ensure;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    typenum::{U1099511627776, U16777216, U2048, U4, U65536, U8192},
    BitVector, FixedVector, VariableList,
};
use tree_hash_derive::TreeHash;

use super::execution_payload_header::ExecutionPayloadHeader;
use crate::{
    beacon_block_header::BeaconBlockHeader,
    checkpoint::Checkpoint,
    eth_1_data::Eth1Data,
    fork::Fork,
    fork_choice::helpers::constants::{
        CHURN_LIMIT_QUOTIENT, EPOCHS_PER_HISTORICAL_VECTOR, GENESIS_EPOCH,
        MIN_PER_EPOCH_CHURN_LIMIT, SLOTS_PER_HISTORICAL_ROOT,
    },
    helpers::is_active_validator,
    historical_summary::HistoricalSummary,
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch},
    sync_committee::SyncCommittee,
    validator::Validator,
};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconState {
    // Versioning
    pub genesis_time: u64,
    pub genesis_validators_root: B256,
    pub slot: u64,
    pub fork: Fork,

    // History
    pub latest_block_header: BeaconBlockHeader,
    pub block_roots: FixedVector<B256, U8192>,
    pub state_roots: FixedVector<B256, U8192>,
    /// Frozen in Capella, replaced by historical_summaries
    pub historical_roots: VariableList<B256, U16777216>,

    // Eth1
    pub eth1_data: Eth1Data,
    pub eth1_data_votes: VariableList<Eth1Data, U2048>,
    pub eth1_deposit_index: u64,

    // Registry
    pub validators: VariableList<Validator, U1099511627776>,
    #[serde(deserialize_with = "ssz_types::serde_utils::quoted_u64_var_list::deserialize")]
    pub balances: VariableList<u64, U1099511627776>,

    // Randomness
    pub randao_mixes: FixedVector<B256, U65536>,

    // Slashings
    #[serde(deserialize_with = "ssz_types::serde_utils::quoted_u64_fixed_vec::deserialize")]
    pub slashings: FixedVector<u64, U8192>,

    // Participation
    pub previous_epoch_participation: VariableList<u8, U1099511627776>,
    pub current_epoch_participation: VariableList<u8, U1099511627776>,

    // Finality
    pub justification_bits: BitVector<U4>,
    pub previous_justified_checkpoint: Checkpoint,
    pub current_justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,

    // Inactivity
    #[serde(deserialize_with = "ssz_types::serde_utils::quoted_u64_var_list::deserialize")]
    pub inactivity_scores: VariableList<u64, U1099511627776>,

    // Sync
    pub current_sync_committee: Arc<SyncCommittee>,
    pub next_sync_committee: Arc<SyncCommittee>,

    // Execution
    pub latest_execution_payload_header: ExecutionPayloadHeader,

    // Withdrawals
    pub next_withdrawal_index: u64,
    pub next_withdrawal_validator_index: u64,

    // Deep history valid from Capella onwards.
    pub historical_summaries: VariableList<HistoricalSummary, U16777216>,
}

impl BeaconState {
    /// Return the current epoch.
    pub fn get_current_epoch(&self) -> u64 {
        compute_epoch_at_slot(self.slot)
    }

    /// Return the previous epoch (unless the current epoch is ``GENESIS_EPOCH``).
    pub fn get_previous_epoch(&self) -> u64 {
        let current_epoch = self.get_current_epoch();
        if current_epoch == GENESIS_EPOCH {
            GENESIS_EPOCH
        } else {
            current_epoch - 1
        }
    }

    /// Return the block root at the start of a recent ``epoch``.
    pub fn get_block_root(&self, epoch: u64) -> anyhow::Result<B256> {
        self.get_block_root_at_slot(compute_start_slot_at_epoch(epoch))
    }

    /// Return the block root at a recent ``slot``.
    pub fn get_block_root_at_slot(&self, slot: u64) -> anyhow::Result<B256> {
        ensure!(
            slot < self.slot && self.slot <= slot + SLOTS_PER_HISTORICAL_ROOT,
            "slot given was outside of block_roots range"
        );
        Ok(self.block_roots[(slot % SLOTS_PER_HISTORICAL_ROOT) as usize])
    }

    /// Return the randao mix at a recent ``epoch``.
    pub fn get_randao_mix(&self, epoch: u64) -> B256 {
        self.randao_mixes[(epoch % EPOCHS_PER_HISTORICAL_VECTOR) as usize]
    }

    /// Return the sequence of active validator indices at ``epoch``.
    pub fn get_active_validator_indices(&self, epoch: u64) -> Vec<u64> {
        self.validators
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                if is_active_validator(v, epoch) {
                    Some(i as u64)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Return the validator churn limit for the current epoch.
    pub fn get_validator_churn_limit(&self) -> u64 {
        let active_validator_indices = self.get_active_validator_indices(self.get_current_epoch());
        max(
            MIN_PER_EPOCH_CHURN_LIMIT,
            active_validator_indices.len() as u64 / CHURN_LIMIT_QUOTIENT,
        )
    }
}
