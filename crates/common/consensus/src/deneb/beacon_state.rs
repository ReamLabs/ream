use std::{
    cmp::{max, min},
    collections::HashSet,
    ops::Deref,
    sync::Arc,
};

use alloy_primitives::{aliases::B32, Address, B256};
use anyhow::{anyhow, bail, ensure};
use blst::min_pk::PublicKey;
use ethereum_hashing::{hash, hash_fixed};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    typenum::{U1099511627776, U16777216, U2048, U4, U65536, U8192},
    BitVector, FixedVector, VariableList,
};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::{
    beacon_block::BeaconBlock, execution_payload::ExecutionPayload,
    execution_payload_header::ExecutionPayloadHeader,
};
use crate::{
    attestation::Attestation,
    attestation_data::AttestationData,
    beacon_block_header::BeaconBlockHeader,
    bls_to_execution_change::SignedBLSToExecutionChange,
    checkpoint::Checkpoint,
    deposit::Deposit,
    deposit_message::DepositMessage,
    eth_1_data::Eth1Data,
    fork::Fork,
    fork_choice::helpers::constants::{
        BASE_REWARD_FACTOR, BLS_WITHDRAWAL_PREFIX, CAPELLA_FORK_VERSION, CHURN_LIMIT_QUOTIENT,
        DEPOSIT_CONTRACT_TREE_DEPTH, DOMAIN_BEACON_ATTESTER, DOMAIN_BEACON_PROPOSER,
        DOMAIN_BLS_TO_EXECUTION_CHANGE, DOMAIN_DEPOSIT, DOMAIN_SYNC_COMMITTEE,
        DOMAIN_VOLUNTARY_EXIT, EFFECTIVE_BALANCE_INCREMENT, EPOCHS_PER_HISTORICAL_VECTOR,
        EPOCHS_PER_SLASHINGS_VECTOR, ETH1_ADDRESS_WITHDRAWAL_PREFIX, FAR_FUTURE_EPOCH,
        GENESIS_EPOCH, GENESIS_SLOT, INACTIVITY_PENALTY_QUOTIENT_ALTAIR, INACTIVITY_SCORE_BIAS,
        INACTIVITY_SCORE_RECOVERY_RATE, MAX_COMMITTEES_PER_SLOT, MAX_EFFECTIVE_BALANCE,
        MAX_RANDOM_BYTE, MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP, MAX_WITHDRAWALS_PER_PAYLOAD,
        MIN_ATTESTATION_INCLUSION_DELAY, MIN_EPOCHS_TO_INACTIVITY_PENALTY,
        MIN_GENESIS_ACTIVE_VALIDATOR_COUNT, MIN_GENESIS_TIME, MIN_PER_EPOCH_CHURN_LIMIT,
        MIN_SEED_LOOKAHEAD, MIN_SLASHING_PENALTY_QUOTIENT, MIN_VALIDATOR_WITHDRAWABILITY_DELAY,
        PROPOSER_REWARD_QUOTIENT, PROPOSER_WEIGHT, SECONDS_PER_SLOT, SHARD_COMMITTEE_PERIOD,
        SLOTS_PER_EPOCH, SLOTS_PER_HISTORICAL_ROOT, SYNC_COMMITTEE_SIZE, TARGET_COMMITTEE_SIZE,
        TIMELY_HEAD_FLAG_INDEX, TIMELY_SOURCE_FLAG_INDEX, TIMELY_TARGET_FLAG_INDEX,
        WEIGHT_DENOMINATOR, WHISTLEBLOWER_REWARD_QUOTIENT,
    },
    helpers::is_active_validator,
    historical_summary::HistoricalSummary,
    indexed_attestation::IndexedAttestation,
    misc::{
        compute_activation_exit_epoch, compute_committee, compute_domain, compute_epoch_at_slot,
        compute_shuffled_index, compute_signing_root, compute_start_slot_at_epoch,
    },
    pubkey::PubKey,
    signature::BlsSignature,
    sync_committee::SyncCommittee,
    validator::Validator,
    voluntary_exit::SignedVoluntaryExit,
    withdrawal::Withdrawal,
};

pub const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_ROPOP";

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
    pub fn get_beacon_proposer_index(&self) -> anyhow::Result<u64> {
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
    pub fn get_total_balance(&self, indices: HashSet<u64>) -> u64 {
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
            self.get_active_validator_indices(self.get_current_epoch())
                .into_iter()
                .collect::<HashSet<_>>(),
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

    /// Increase the validator balance at index ``index`` by ``delta``.
    pub fn increase_balance(&mut self, index: u64, delta: u64) {
        if let Some(balance) = self.balances.get_mut(index as usize) {
            *balance += delta;
        }
    }

    /// Decrease the validator balance at index ``index`` by ``delta`` with underflow protection.
    pub fn decrease_balance(&mut self, index: u64, delta: u64) {
        if let Some(balance) = self.balances.get_mut(index as usize) {
            let _ = balance.saturating_sub(delta);
        }
    }

    /// Initiate if validator already initiated exit.
    pub fn initiate_validator_exit(&mut self, index: u64) {
        if index as usize >= self.validators.len() {
            return;
        }
        if self
            .validators
            .get(index as usize)
            .expect("Can't find index in validators")
            .exit_epoch
            != FAR_FUTURE_EPOCH
        {
            return;
        }

        let mut exit_epochs: Vec<u64> = self
            .validators
            .iter()
            .filter_map(|v| {
                if v.exit_epoch != FAR_FUTURE_EPOCH {
                    Some(v.exit_epoch)
                } else {
                    None
                }
            })
            .collect();

        exit_epochs.push(compute_activation_exit_epoch(self.get_current_epoch()));
        let mut exit_queue_epoch = *exit_epochs.iter().max().unwrap_or(&0);

        let exit_queue_churn = self
            .validators
            .iter()
            .filter(|v| v.exit_epoch == exit_queue_epoch)
            .count();

        if exit_queue_churn >= self.get_validator_churn_limit() as usize {
            exit_queue_epoch += 1;
        }

        // Set validator exit epoch and withdrawable epoch
        if let Some(validator) = self.validators.get_mut(index as usize) {
            validator.exit_epoch = exit_queue_epoch;
            validator.withdrawable_epoch =
                validator.exit_epoch + MIN_VALIDATOR_WITHDRAWABILITY_DELAY;
        }
    }

    /// Slash the validator with index ``slashed_index``
    pub fn slash_validator(
        &mut self,
        slashed_index: u64,
        whistleblower_index: Option<u64>,
    ) -> anyhow::Result<()> {
        let epoch = self.get_current_epoch();

        // Initiate validator exit
        self.initiate_validator_exit(slashed_index);

        let validator_effective_balance =
            if let Some(validator) = self.validators.get_mut(slashed_index as usize) {
                validator.slashed = true;
                validator.withdrawable_epoch = std::cmp::max(
                    validator.withdrawable_epoch,
                    epoch + EPOCHS_PER_SLASHINGS_VECTOR,
                );
                validator.effective_balance
            } else {
                bail!("Validator at index {slashed_index} not found")
            };
        // Add slashed effective balance to the slashings vector
        self.slashings[(epoch % EPOCHS_PER_SLASHINGS_VECTOR) as usize] +=
            validator_effective_balance;
        // Decrease validator balance
        self.decrease_balance(
            slashed_index,
            validator_effective_balance / MIN_SLASHING_PENALTY_QUOTIENT,
        );

        // Apply proposer and whistleblower rewards
        let proposer_index = self.get_beacon_proposer_index()?;
        let whistleblower_index = whistleblower_index.unwrap_or(proposer_index);

        let whistleblower_reward = validator_effective_balance / WHISTLEBLOWER_REWARD_QUOTIENT;
        let proposer_reward = whistleblower_reward * PROPOSER_WEIGHT / WEIGHT_DENOMINATOR;
        self.increase_balance(proposer_index, proposer_reward);
        self.increase_balance(whistleblower_index, whistleblower_reward - proposer_reward);

        Ok(())
    }
    pub fn is_valid_genesis_state(&self) -> bool {
        if self.genesis_time < MIN_GENESIS_TIME {
            return false;
        }
        if self.get_active_validator_indices(GENESIS_EPOCH).len()
            < MIN_GENESIS_ACTIVE_VALIDATOR_COUNT as usize
        {
            return false;
        }
        true
    }

    pub fn add_flag(flags: u8, flag_index: u8) -> u8 {
        let flag = 2 << flag_index;
        flags | flag
    }

    pub fn has_flag(flags: u8, flag_index: u8) -> bool {
        let flag = 2 << flag_index;
        flags & flag == flag
    }

    pub fn get_unslashed_participating_indices(
        &self,
        flag_index: u8,
        epoch: u64,
    ) -> anyhow::Result<HashSet<u64>> {
        ensure!(
            epoch == self.get_previous_epoch() || epoch == self.get_current_epoch(),
            "Epoch must be either the previous or current epoch"
        );
        let epoch_participation = if epoch == self.get_current_epoch() {
            &self.current_epoch_participation
        } else {
            &self.previous_epoch_participation
        };
        let active_validator_indices = self.get_active_validator_indices(epoch);
        let mut participating_indices = vec![];
        for i in active_validator_indices {
            if Self::has_flag(epoch_participation[i as usize], flag_index) {
                participating_indices.push(i);
            }
        }
        let filtered_indices: HashSet<u64> = participating_indices
            .into_iter()
            .filter(|&index| self.validators[index as usize].slashed)
            .collect();
        Ok(filtered_indices)
    }

    pub fn process_inactivity_updates(&mut self) -> anyhow::Result<()> {
        // Skip the genesis epoch as score updates are based on the previous epoch participation
        if self.get_current_epoch() == GENESIS_EPOCH {
            return Ok(());
        }
        for index in self.get_eligible_validator_indices()? {
            // Increase the inactivity score of inactive validators
            if self
                .get_unslashed_participating_indices(
                    TIMELY_TARGET_FLAG_INDEX,
                    self.get_previous_epoch(),
                )?
                .contains(&index)
            {
                self.inactivity_scores[index as usize] -=
                    min(1, self.inactivity_scores[index as usize])
            } else {
                self.inactivity_scores[index as usize] += INACTIVITY_SCORE_BIAS
            }

            // Decrease the inactivity score of all eligible validators during a leak-free epoch
            if !self.is_in_inactivity_leak() {
                self.inactivity_scores[index as usize] -= min(
                    INACTIVITY_SCORE_RECOVERY_RATE,
                    self.inactivity_scores[index as usize],
                )
            }
        }
        Ok(())
    }

    pub fn get_base_reward_per_increment(&self) -> u64 {
        EFFECTIVE_BALANCE_INCREMENT * BASE_REWARD_FACTOR
            / (self.get_total_active_balance() as f64).sqrt() as u64
    }

    /// Return the base reward for the validator defined by ``index`` with respect to the current
    /// ``state``.
    pub fn get_base_reward(&self, index: u64) -> u64 {
        let increments =
            self.validators[index as usize].effective_balance / EFFECTIVE_BALANCE_INCREMENT;
        increments * self.get_base_reward_per_increment()
    }

    pub fn get_proposer_reward(&self, attesting_index: u64) -> u64 {
        self.get_base_reward(attesting_index) / PROPOSER_REWARD_QUOTIENT
    }

    pub fn get_finality_delay(&self) -> u64 {
        self.get_previous_epoch() - self.finalized_checkpoint.epoch
    }

    pub fn is_in_inactivity_leak(&self) -> bool {
        self.get_finality_delay() > MIN_EPOCHS_TO_INACTIVITY_PENALTY
    }

    pub fn get_eligible_validator_indices(&self) -> anyhow::Result<Vec<u64>> {
        let previous_epoch = self.get_previous_epoch();
        let mut validator_indices = vec![];
        for (index, v) in self.validators.iter().enumerate() {
            if is_active_validator(v, previous_epoch)
                || v.slashed && previous_epoch + 1 < v.withdrawable_epoch
            {
                validator_indices.push(index as u64)
            }
        }
        Ok(validator_indices)
    }

    pub fn get_index_for_new_validator(&self) -> u64 {
        self.validators.len() as u64
    }

    /// Return the flag indices that are satisfied by an attestation.
    pub fn get_attestation_participation_flag_indices(
        &self,
        data: AttestationData,
        inclusion_delay: u64,
    ) -> anyhow::Result<Vec<u8>> {
        let justified_checkpoint = if data.target.epoch == self.get_current_epoch() {
            self.current_justified_checkpoint
        } else {
            self.previous_justified_checkpoint
        };
        let is_matching_source = data.source == justified_checkpoint;
        let is_matching_target =
            is_matching_source && data.target.root == self.get_block_root(data.target.epoch)?;
        let is_matching_head = is_matching_target
            && data.beacon_block_root == self.get_block_root_at_slot(data.slot)?;
        ensure!(is_matching_source);

        let mut participation_flag_indices = vec![];

        if is_matching_source && inclusion_delay <= (SLOTS_PER_EPOCH as f64).sqrt() as u64 {
            participation_flag_indices.push(TIMELY_SOURCE_FLAG_INDEX);
        }
        if is_matching_target && inclusion_delay <= SLOTS_PER_EPOCH {
            participation_flag_indices.push(TIMELY_TARGET_FLAG_INDEX);
        }
        if is_matching_head && inclusion_delay == MIN_ATTESTATION_INCLUSION_DELAY {
            participation_flag_indices.push(TIMELY_HEAD_FLAG_INDEX);
        }

        Ok(participation_flag_indices)
    }

    pub fn get_inactivity_penalty_deltas(&self) -> anyhow::Result<(Vec<u64>, Vec<u64>)> {
        let rewards = vec![0; self.validators.len()];
        let mut penalties = vec![0; self.validators.len()];
        let previous_epoch = self.get_previous_epoch();
        let matching_target_indices =
            self.get_unslashed_participating_indices(TIMELY_TARGET_FLAG_INDEX, previous_epoch)?;
        for index in self.get_eligible_validator_indices()? {
            if !matching_target_indices.contains(&index) {
                let penalty_numerator = self.validators[index as usize].effective_balance
                    * self.inactivity_scores[index as usize];
                let penalty_denominator =
                    INACTIVITY_SCORE_BIAS * INACTIVITY_PENALTY_QUOTIENT_ALTAIR;
                penalties[index as usize] += penalty_numerator / penalty_denominator
            }
        }
        Ok((rewards, penalties))
    }

    pub fn process_block_header(&mut self, block: BeaconBlock) -> anyhow::Result<()> {
        // Verify that the slots match
        ensure!(
            self.slot == block.slot,
            "State slot must be equal to block slot"
        );
        // Verify that the block is newer than latest block header
        ensure!(
            block.slot > self.latest_block_header.slot,
            "Block slot must be greater than latest block header slot of state"
        );
        // Verify that proposer index is the correct index
        ensure!(
            block.proposer_index == self.get_beacon_proposer_index()?,
            "Block proposer index must be equal to beacon proposer index"
        );
        // Verify that the parent matches
        ensure!(
            block.parent_root == self.latest_block_header.tree_hash_root(),
            "Block Parent Root must be equal root of latest block header"
        );

        // Cache current block as the new latest block
        self.latest_block_header = BeaconBlockHeader {
            slot: block.slot,
            proposer_index: block.proposer_index,
            parent_root: block.parent_root,
            state_root: B256::default(), // Overwritten in the next process_slot call
            body_root: block.body.tree_hash_root(),
        };

        // Verify proposer is not slashed
        let proposer = &self.validators[block.proposer_index as usize];
        ensure!(!proposer.slashed, "Block proposer must not be slashed");

        Ok(())
    }

    pub fn get_expected_withdrawals(&self) -> Vec<Withdrawal> {
        let epoch = self.get_current_epoch();
        let mut withdrawal_index = self.next_withdrawal_index;
        let mut validator_index = self.next_withdrawal_validator_index;
        let mut withdrawals: Vec<Withdrawal> = vec![];
        let bound = min(self.validators.len(), MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP);
        for _ in 0..bound {
            let validator = &self.validators[validator_index as usize];
            let balance = self.balances[validator_index as usize];
            if validator.is_fully_withdrawable_validator(balance, epoch) {
                withdrawals.push(Withdrawal {
                    index: withdrawal_index,
                    validator_index,
                    address: Address::from_slice(&validator.withdrawal_credentials[..12]),
                    amount: balance,
                });
                withdrawal_index += 1
            } else if validator.is_partially_withdrawable_validator(balance) {
                withdrawals.push(Withdrawal {
                    index: withdrawal_index,
                    validator_index,
                    address: Address::from_slice(&validator.withdrawal_credentials[..12]),
                    amount: balance - MAX_EFFECTIVE_BALANCE,
                });
                withdrawal_index += 1
            }
            if withdrawals.len() == MAX_WITHDRAWALS_PER_PAYLOAD as usize {
                break;
            }
            validator_index = (validator_index + 1) % self.validators.len() as u64
        }
        withdrawals
    }

    pub fn process_withdrawals(&mut self, payload: ExecutionPayload) -> anyhow::Result<()> {
        let expected_withdrawals = self.get_expected_withdrawals();
        ensure!(
            payload.withdrawals.deref() == expected_withdrawals,
            "Can't compare"
        );

        for withdrawal in &expected_withdrawals {
            self.decrease_balance(withdrawal.validator_index, withdrawal.amount);
        }

        // Update the next withdrawal index if this block contained withdrawals
        if expected_withdrawals.is_empty() {
            let latest_withdrawal = &expected_withdrawals[expected_withdrawals.len() - 1];
            self.next_withdrawal_index = latest_withdrawal.index + 1
        }

        // Update the next validator index to start the next withdrawal sweep
        if expected_withdrawals.len() == MAX_WITHDRAWALS_PER_PAYLOAD as usize {
            // Next sweep starts after the latest withdrawal's validator index
            let next_validator_index = expected_withdrawals[expected_withdrawals.len() - 1]
                .validator_index
                + 1 % self.validators.len() as u64;
            self.next_withdrawal_validator_index = next_validator_index
        } else {
            // Advance sweep by the max length of the sweep if there was not a full set of
            // withdrawals
            let next_index =
                self.next_withdrawal_validator_index + MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP as u64;
            let next_validator_index = next_index % self.validators.len() as u64;
            self.next_withdrawal_validator_index = next_validator_index
        }

        Ok(())
    }

    pub fn add_validator_to_registry(
        &mut self,
        pubkey: PubKey,
        withdrawal_credentials: B256,
        amount: u64,
    ) -> anyhow::Result<()> {
        self.validators
            .push(get_validator_from_deposit(
                pubkey,
                withdrawal_credentials,
                amount,
            ))
            .map_err(|err| anyhow!("Couldn't push to validators {:?}", err))?;
        self.balances
            .push(amount)
            .map_err(|err| anyhow!("Couldn't push to balances {:?}", err))?;
        Ok(())
    }

    pub fn apply_deposit(
        &mut self,
        pubkey: PubKey,
        withdrawal_credentials: B256,
        amount: u64,
        signature: BlsSignature,
    ) -> anyhow::Result<()> {
        let mut validator_pubkeys = vec![];
        for validator in &self.validators {
            validator_pubkeys.push(validator.pubkey.clone());
        }
        if !validator_pubkeys.contains(&pubkey) {
            // Verify the deposit signature (proof of possession) which is not checked by the
            // deposit contract
            let deposit_message = DepositMessage {
                pubkey: pubkey.clone(),
                withdrawal_credentials,
                amount,
            };
            let domain = compute_domain(DOMAIN_DEPOSIT, None, None)?; // # Fork-agnostic domain since deposits are valid across forks
            let signing_root = compute_signing_root(deposit_message, domain);
            let sig = blst::min_pk::Signature::from_bytes(&signature.signature)
                .map_err(|err| anyhow!("Failed to convert signiture type {err:?}"))?;
            let public_key = PublicKey::from_bytes(&pubkey.inner)
                .map_err(|err| anyhow!("Failed to convert pubkey type {err:?}"))?;
            let verification_result =
                sig.fast_aggregate_verify(true, signing_root.as_ref(), DST, &[&public_key]);
            if verification_result == blst::BLST_ERROR::BLST_SUCCESS {
                self.add_validator_to_registry(pubkey, withdrawal_credentials, amount)?;
            }
        } else {
            // Increase balance by deposit amount
            let index = validator_pubkeys
                .iter()
                .position(|r| *r == pubkey)
                .ok_or(anyhow!("Can't find pubkey in validator_pubkeys"))?;
            self.increase_balance(index as u64, amount);
        }
        Ok(())
    }

    pub fn process_deposit(&mut self, deposit: Deposit) -> anyhow::Result<()> {
        // Verify the Merkle branch
        ensure!(is_valid_merkle_branch(
            deposit.data.tree_hash_root(),
            &deposit.proof,
            DEPOSIT_CONTRACT_TREE_DEPTH + 1, // Add 1 for the List length mix-in
            self.eth1_deposit_index,
            self.eth1_data.deposit_root,
        ));

        // Deposits must be processed in order
        self.eth1_deposit_index += 1;

        self.apply_deposit(
            deposit.data.pubkey,
            deposit.data.withdrawal_credentials,
            deposit.data.amount,
            deposit.data.signature,
        )
    }

    pub fn process_bls_to_execution_change(
        &mut self,
        signed_address_change: SignedBLSToExecutionChange,
    ) -> anyhow::Result<()> {
        let address_change = signed_address_change.message;

        ensure!(address_change.validator_index < self.validators.len() as u64);

        let validator: &Validator = &self.validators[address_change.validator_index as usize];

        ensure!(&validator.withdrawal_credentials[..1] == BLS_WITHDRAWAL_PREFIX);
        ensure!(
            validator.withdrawal_credentials[1..]
                == hash(&address_change.from_bls_pubkey.inner)[1..]
        );

        // Fork-agnostic domain since address changes are valid across forks
        let domain = compute_domain(
            DOMAIN_BLS_TO_EXECUTION_CHANGE,
            None,
            Some(self.genesis_validators_root),
        )?;

        let signing_root = compute_signing_root(&address_change, domain);
        let sig = blst::min_pk::Signature::from_bytes(&signed_address_change.signature.signature)
            .map_err(|err| anyhow!("Failed to convert signiture type {err:?}"))?;
        let public_key = PublicKey::from_bytes(&address_change.from_bls_pubkey.inner)
            .map_err(|err| anyhow!("Failed to convert pubkey type {err:?}"))?;
        let verification_result =
            sig.fast_aggregate_verify(true, signing_root.as_ref(), DST, &[&public_key]);
        ensure!(
            verification_result == blst::BLST_ERROR::BLST_SUCCESS,
            "BLS Signature verification failed!"
        );

        let withdrawal_credentials = [
            ETH1_ADDRESS_WITHDRAWAL_PREFIX.as_slice(),
            vec![0x00; 11].as_slice(),
            address_change.to_execution_address.as_slice(),
        ]
        .concat();
        self.validators[address_change.validator_index as usize].withdrawal_credentials =
            B256::from_slice(&withdrawal_credentials);

        Ok(())
    }

    pub fn compute_timestamp_at_slot(&self, slot: u64) -> u64 {
        let slots_since_genesis = slot - GENESIS_SLOT;
        self.genesis_time + slots_since_genesis * SECONDS_PER_SLOT
    }

    pub fn process_voluntary_exit(
        &mut self,
        signed_voluntary_exit: SignedVoluntaryExit,
    ) -> anyhow::Result<()> {
        let voluntary_exit = &signed_voluntary_exit.message;
        let validator_index = voluntary_exit.validator_index as usize;

        let validator = self
            .validators
            .get(validator_index)
            .ok_or(anyhow!("Invalid validator index"))?;

        // Verify the validator is active
        ensure!(
            is_active_validator(validator, self.get_current_epoch()),
            "Validator is not active"
        );

        // Verify exit has not been initiated
        ensure!(
            validator.exit_epoch == FAR_FUTURE_EPOCH,
            "Exit has already been initiated"
        );

        // Exits must specify an epoch when they become valid; they are not valid before then
        ensure!(
            self.get_current_epoch() >= voluntary_exit.epoch,
            "Exit is not yet valid"
        );

        // Verify the validator has been active long enough
        ensure!(
            self.get_current_epoch() >= validator.activation_epoch + SHARD_COMMITTEE_PERIOD,
            "Validator has not been active long enough"
        );

        // Compute signature domain
        let domain = compute_domain(
            DOMAIN_VOLUNTARY_EXIT,
            Some(CAPELLA_FORK_VERSION),
            Some(self.genesis_validators_root),
        )?;
        let signing_root = compute_signing_root(voluntary_exit, domain);

        let sig = blst::min_pk::Signature::from_bytes(&signed_voluntary_exit.signature.signature)
            .map_err(|err| {
            anyhow!("Failed to convert signature to blst Signature type, {err:?}")
        })?;
        let public_key = PublicKey::from_bytes(&validator.pubkey.inner)
            .map_err(|err| anyhow!("Failed to convert pubkey to blst PublicKey type, {err:?}"))?;

        let verification_result =
            sig.fast_aggregate_verify(true, signing_root.as_ref(), DST, &[&public_key]);

        ensure!(
            verification_result == blst::BLST_ERROR::BLST_SUCCESS,
            "BLS Signature verification failed!"
        );

        // Initiate exit
        self.initiate_validator_exit(validator_index as u64);

        Ok(())
    }

    /// Return the sync committee indices, with possible duplicates, for the next sync committee.
    pub fn get_next_sync_committee_indices(&self) -> anyhow::Result<Vec<u64>> {
        let epoch = self.get_current_epoch() + 1;
        let active_validator_indices = self.get_active_validator_indices(epoch);
        let active_validator_count = active_validator_indices.len();
        let seed = self.get_seed(epoch, DOMAIN_SYNC_COMMITTEE);
        let mut i = 0;
        let mut sync_committee_indices: Vec<u64> = vec![];
        while sync_committee_indices.len() < SYNC_COMMITTEE_SIZE as usize {
            let shuffled_index =
                compute_shuffled_index(i % active_validator_count, active_validator_count, seed)?;
            let candidate_index = active_validator_indices[shuffled_index];
            let seed_with_index = [seed.as_slice(), &(i / 32).to_le_bytes()].concat();
            let hash = hash(&seed_with_index);
            let random_byte = hash[i % 32];
            let effective_balance = self.validators[candidate_index as usize].effective_balance;
            if effective_balance * MAX_RANDOM_BYTE >= MAX_EFFECTIVE_BALANCE * random_byte as u64 {
                sync_committee_indices.push(candidate_index)
            }
            i += 1
        }

        Ok(sync_committee_indices)
    }
}

/// Check if ``leaf`` at ``index`` verifies against the Merkle ``root`` and ``branch``.
pub fn is_valid_merkle_branch(
    leaf: B256,
    branch: &[B256],
    depth: u64,
    index: u64,
    root: B256,
) -> bool {
    let mut value = leaf;
    for i in 0..depth {
        if (index / 2u64.pow(i as u32) % 2) == 1 {
            let branch_value = [branch[i as usize], value].concat();
            value = B256::from_slice(&hash(&branch_value));
        } else {
            let branch_value = [value, branch[i as usize]].concat();
            value = B256::from_slice(&hash(&branch_value));
        }
    }
    value == root
}

pub fn get_validator_from_deposit(
    pubkey: PubKey,
    withdrawal_credentials: B256,
    amount: u64,
) -> Validator {
    let effective_balance = min(
        amount - amount % EFFECTIVE_BALANCE_INCREMENT,
        MAX_EFFECTIVE_BALANCE,
    );
    Validator {
        pubkey,
        withdrawal_credentials,
        effective_balance,
        slashed: false,
        activation_eligibility_epoch: FAR_FUTURE_EPOCH,
        activation_epoch: FAR_FUTURE_EPOCH,
        exit_epoch: FAR_FUTURE_EPOCH,
        withdrawable_epoch: FAR_FUTURE_EPOCH,
    }
}
