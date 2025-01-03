use std::sync::Arc;

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    typenum::{U1099511627776, U16777216, U2048, U4, U65536, U8192},
    BitVector, FixedVector, VariableList,
};
use tree_hash_derive::TreeHash;

use super::execution_payload_header::ExecutionPayloadHeader;
use crate::{
    beacon_block_header::BeaconBlockHeader, checkpoint::Checkpoint, eth_1_data::Eth1Data,
    fork::Fork, fork_choice::helpers::misc::compute_epoch_at_slot,
    historical_summary::HistoricalSummary, sync_committee::SyncCommittee, validator::Validator,
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
    pub fn get_current_epoch(&self) -> u64 {
        compute_epoch_at_slot(self.slot)
    }
}
