use std::{cmp::max, sync::Arc};

use alloy_primitives::{aliases::B32, B256};
use anyhow::ensure;
use ethereum_hashing::{hash, hash_fixed};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    typenum::{U1099511627776, U16777216, U2048, U4, U65536, U8192},
    BitVector, FixedVector, VariableList,
};
use tree_hash_derive::TreeHash;

use super::execution_payload_header::ExecutionPayloadHeader;
use crate::{
    attestation::Attestation,
    beacon_block_header::BeaconBlockHeader,
    checkpoint::Checkpoint,
    eth_1_data::Eth1Data,
    fork::Fork,
    fork_choice::helpers::constants::{
        CHURN_LIMIT_QUOTIENT, DOMAIN_BEACON_ATTESTER, DOMAIN_BEACON_PROPOSER,
        EFFECTIVE_BALANCE_INCREMENT, EPOCHS_PER_HISTORICAL_VECTOR, GENESIS_EPOCH,
        MAX_COMMITTEES_PER_SLOT, MAX_EFFECTIVE_BALANCE, MAX_RANDOM_BYTE, MIN_PER_EPOCH_CHURN_LIMIT,
        MIN_SEED_LOOKAHEAD, SLOTS_PER_EPOCH, SLOTS_PER_HISTORICAL_ROOT, TARGET_COMMITTEE_SIZE,
    },
    helpers::is_active_validator,
    historical_summary::HistoricalSummary,
    indexed_attestation::IndexedAttestation,
    misc::{
        compute_committee, compute_domain, compute_epoch_at_slot, compute_shuffled_index,
        compute_start_slot_at_epoch,
    },
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

    /// Return the seed at ``epoch``.
    pub fn get_seed(&self, epoch: u64, domain_type: B32) -> B256 {
        let mix =
            self.get_randao_mix(epoch + EPOCHS_PER_HISTORICAL_VECTOR - MIN_SEED_LOOKAHEAD - 1);
        let epoch_with_index =
            [domain_type.as_slice(), &epoch.to_le_bytes(), mix.as_slice()].concat();
        B256::from(hash_fixed(&epoch_with_index))
    }

    /// Return the number of committees in each slot for the given ``epoch``.
    pub fn get_committee_count_per_slot(&self, epoch: u64) -> u64 {
        (self.get_active_validator_indices(epoch).len() as u64
            / SLOTS_PER_EPOCH
            / TARGET_COMMITTEE_SIZE)
            .clamp(1, MAX_COMMITTEES_PER_SLOT)
    }

    /// Return from ``indices`` a random index sampled by effective balance
    pub fn compute_proposer_index(&self, indices: &[u64], seed: B256) -> anyhow::Result<u64> {
        ensure!(!indices.is_empty(), "Index must be less than index_count");

        let mut i: usize = 0;
        let total = indices.len();

        loop {
            let candidate_index = indices[compute_shuffled_index(i % total, total, seed)?];

            let seed_with_index = [seed.as_slice(), &(i / 32).to_le_bytes()].concat();
            let hash = hash(&seed_with_index);
            let random_byte = hash[i % 32];

            let effective_balance = self.validators[candidate_index as usize].effective_balance;

            if (effective_balance * MAX_RANDOM_BYTE) >= (MAX_EFFECTIVE_BALANCE * random_byte as u64)
            {
                return Ok(candidate_index);
            }

            i += 1;
        }
    }

    /// Return the beacon proposer index at the current slot.
    pub fn get_beacon_proposer_index(&self) -> Result<u64, anyhow::Error> {
        let epoch = self.get_current_epoch();
        let seed = B256::from(hash_fixed(
            &[
                self.get_seed(epoch, DOMAIN_BEACON_PROPOSER).as_slice(),
                &self.slot.to_le_bytes(),
            ]
            .concat(),
        ));
        let indices = self.get_active_validator_indices(epoch);
        self.compute_proposer_index(&indices, seed)
    }

    /// Return the combined effective balance of the ``indices``.
    /// ``EFFECTIVE_BALANCE_INCREMENT`` Gwei minimum to avoid divisions by zero.
    /// Math safe up to ~10B ETH, after which this overflows uint64.
    pub fn get_total_balance(&self, indices: &[u64]) -> u64 {
        max(
            EFFECTIVE_BALANCE_INCREMENT,
            indices
                .iter()
                .map(|index| self.validators[*index as usize].effective_balance)
                .sum(),
        )
    }

    /// Return the combined effective balance of the active validators.
    /// Note: ``get_total_balance`` returns ``EFFECTIVE_BALANCE_INCREMENT`` Gwei minimum to avoid
    /// divisions by zero.
    pub fn get_total_active_balance(&self) -> u64 {
        self.get_total_balance(
            &self
                .get_active_validator_indices(self.get_current_epoch())
                .into_iter()
                .unique()
                .collect::<Vec<_>>(),
        )
    }

    /// Return the signature domain (fork version concatenated with domain type) of a message.
    pub fn get_domain(&self, domain_type: B32, epoch: Option<u64>) -> anyhow::Result<B256> {
        let epoch = match epoch {
            Some(epoch) => epoch,
            None => self.get_current_epoch(),
        };
        let fork_version = if epoch < self.fork.epoch {
            self.fork.previous_version
        } else {
            self.fork.current_version
        };
        compute_domain(
            domain_type,
            Some(fork_version),
            Some(self.genesis_validators_root),
        )
    }

    /// Return the beacon committee at ``slot`` for ``index``.
    pub fn get_beacon_committee(&self, slot: u64, index: u64) -> anyhow::Result<Vec<u64>> {
        let epoch = compute_epoch_at_slot(slot);
        let committees_per_slot = self.get_committee_count_per_slot(epoch);
        compute_committee(
            &self.get_active_validator_indices(epoch),
            self.get_seed(epoch, DOMAIN_BEACON_ATTESTER),
            (slot % SLOTS_PER_EPOCH) * committees_per_slot + index,
            committees_per_slot * SLOTS_PER_EPOCH,
        )
    }

    /// Return the set of attesting indices corresponding to ``data`` and ``bits``.
    pub fn get_attesting_indices(&self, attestation: Attestation) -> anyhow::Result<Vec<u64>> {
        let committee = self.get_beacon_committee(attestation.data.slot, attestation.data.index)?;
        let indices: Vec<u64> = committee
            .into_iter()
            .enumerate()
            .filter_map(|(i, index)| {
                attestation
                    .aggregation_bits
                    .get(i)
                    .ok()
                    .filter(|&bit| bit)
                    .map(|_| index)
            })
            .unique()
            .collect();
        Ok(indices)
    }

    /// Return the indexed attestation corresponding to ``attestation``.
    pub fn get_indexed_attestation(
        &self,
        attestation: Attestation,
    ) -> anyhow::Result<IndexedAttestation> {
        let mut attesting_indices = self.get_attesting_indices(attestation.clone())?;
        attesting_indices.sort();
        Ok(IndexedAttestation {
            attesting_indices: attesting_indices.into(),
            data: attestation.data,
            signature: attestation.signature,
        })
    }
}
