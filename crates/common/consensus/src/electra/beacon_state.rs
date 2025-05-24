use std::{
    cmp::{max, min},
    collections::HashSet,
    mem::take,
    ops::Deref,
    sync::Arc,
};

use alloy_primitives::{Address, B256, aliases::B32};
use anyhow::{anyhow, bail, ensure};
use ethereum_hashing::{hash, hash_fixed};
use itertools::Itertools;
use ream_bls::{
    AggregatePubKey, BLSSignature, PubKey,
    traits::{Aggregatable, Verifiable},
};
use ream_merkle::{generate_proof, is_valid_merkle_branch, merkle_tree};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    BitVector, FixedVector, VariableList,
    serde_utils::{quoted_u64_fixed_vec, quoted_u64_var_list},
    typenum::{U4, U2048, U8192, U65536, U262144, U16777216, U134217728, U1099511627776},
};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::{
    beacon_block::{BeaconBlock, SignedBeaconBlock},
    beacon_block_body::BeaconBlockBody,
    execution_payload::ExecutionPayload,
    execution_payload_header::ExecutionPayloadHeader,
};
use crate::{
    attestation::Attestation,
    attestation_data::AttestationData,
    attester_slashing::AttesterSlashing,
    beacon_block_header::BeaconBlockHeader,
    bls_to_execution_change::SignedBLSToExecutionChange,
    checkpoint::Checkpoint,
    consolidation_request::ConsolidationRequest,
    constants::{
        BASE_REWARD_FACTOR, BEACON_STATE_MERKLE_DEPTH, BLS_WITHDRAWAL_PREFIX, CAPELLA_FORK_VERSION,
        CHURN_LIMIT_QUOTIENT, COMPOUNDING_WITHDRAWAL_PREFIX, CURRENT_SYNC_COMMITTEE_INDEX,
        DEPOSIT_CONTRACT_TREE_DEPTH, DOMAIN_BEACON_ATTESTER, DOMAIN_BEACON_PROPOSER,
        DOMAIN_BLS_TO_EXECUTION_CHANGE, DOMAIN_DEPOSIT, DOMAIN_RANDAO, DOMAIN_SYNC_COMMITTEE,
        DOMAIN_VOLUNTARY_EXIT, EFFECTIVE_BALANCE_INCREMENT, EJECTION_BALANCE,
        EPOCHS_PER_ETH1_VOTING_PERIOD, EPOCHS_PER_HISTORICAL_VECTOR, EPOCHS_PER_SLASHINGS_VECTOR,
        EPOCHS_PER_SYNC_COMMITTEE_PERIOD, ETH1_ADDRESS_WITHDRAWAL_PREFIX, FAR_FUTURE_EPOCH,
        FINALIZED_CHECKPOINT_INDEX, FULL_EXIT_REQUEST_AMOUNT, GENESIS_EPOCH, GENESIS_SLOT,
        HYSTERESIS_DOWNWARD_MULTIPLIER, HYSTERESIS_QUOTIENT, HYSTERESIS_UPWARD_MULTIPLIER,
        INACTIVITY_PENALTY_QUOTIENT_BELLATRIX, INACTIVITY_SCORE_BIAS,
        INACTIVITY_SCORE_RECOVERY_RATE, JUSTIFICATION_BITS_LENGTH, MAX_BLOBS_PER_BLOCK_ELECTRA,
        MAX_COMMITTEES_PER_SLOT, MAX_DEPOSITS, MAX_EFFECTIVE_BALANCE_ELECTRA,
        MAX_PENDING_DEPOSITS_PER_EPOCH, MAX_PENDING_PARTIALS_PER_WITHDRAWALS_SWEEP,
        MAX_PER_EPOCH_ACTIVATION_CHURN_LIMIT, MAX_PER_EPOCH_ACTIVATION_EXIT_CHURN_LIMIT,
        MAX_RANDOM_VALUE, MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP, MAX_WITHDRAWALS_PER_PAYLOAD,
        MIN_ACTIVATION_BALANCE, MIN_ATTESTATION_INCLUSION_DELAY, MIN_EPOCHS_TO_INACTIVITY_PENALTY,
        MIN_GENESIS_ACTIVE_VALIDATOR_COUNT, MIN_GENESIS_TIME, MIN_PER_EPOCH_CHURN_LIMIT,
        MIN_PER_EPOCH_CHURN_LIMIT_ELECTRA, MIN_SEED_LOOKAHEAD,
        MIN_SLASHING_PENALTY_QUOTIENT_ELECTRA, MIN_VALIDATOR_WITHDRAWABILITY_DELAY,
        NEXT_SYNC_COMMITTEE_INDEX, PARTICIPATION_FLAG_WEIGHTS, PENDING_CONSOLIDATIONS_LIMIT,
        PENDING_PARTIAL_WITHDRAWALS_LIMIT, PROPORTIONAL_SLASHING_MULTIPLIER_BELLATRIX,
        PROPOSER_REWARD_QUOTIENT, PROPOSER_WEIGHT, SECONDS_PER_SLOT, SHARD_COMMITTEE_PERIOD,
        SLOTS_PER_EPOCH, SLOTS_PER_HISTORICAL_ROOT, SYNC_COMMITTEE_SIZE, SYNC_REWARD_WEIGHT,
        TARGET_COMMITTEE_SIZE, TIMELY_HEAD_FLAG_INDEX, TIMELY_SOURCE_FLAG_INDEX,
        TIMELY_TARGET_FLAG_INDEX, UINT64_MAX, UINT64_MAX_SQRT, UNSET_DEPOSIT_REQUESTS_START_INDEX,
        WEIGHT_DENOMINATOR, WHISTLEBLOWER_REWARD_QUOTIENT_ELECTRA,
    },
    deposit::Deposit,
    deposit_message::DepositMessage,
    deposit_request::DepositRequest,
    eth_1_data::Eth1Data,
    execution_engine::{engine_trait::ExecutionApi, new_payload_request::NewPayloadRequest},
    fork::Fork,
    helpers::xor,
    historical_summary::HistoricalSummary,
    indexed_attestation::IndexedAttestation,
    misc::{
        bytes_to_int64, compute_activation_exit_epoch, compute_committee, compute_domain,
        compute_epoch_at_slot, compute_shuffled_index, compute_signing_root,
        compute_start_slot_at_epoch, get_committee_indices, is_sorted_and_unique,
    },
    pending_consolidation::PendingConsolidation,
    pending_deposit::PendingDeposit,
    pending_partial_withdrawal::PendingPartialWithdrawal,
    predicates::is_slashable_attestation_data,
    proposer_slashing::ProposerSlashing,
    sync_aggregate::SyncAggregate,
    sync_committee::SyncCommittee,
    validator::Validator,
    voluntary_exit::SignedVoluntaryExit,
    withdrawal::Withdrawal,
    withdrawal_request::WithdrawalRequest,
};

pub mod quoted_u8_var_list {
    use super::*;

    pub fn serialize<S>(
        value: &VariableList<u8, U1099511627776>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let string_vec: Vec<String> = value.iter().map(|v| v.to_string()).collect();
        string_vec.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<VariableList<u8, U1099511627776>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string_vec: Vec<String> = Vec::deserialize(deserializer)?;
        let bytes = string_vec
            .into_iter()
            .map(|s| s.parse::<u8>().map_err(serde::de::Error::custom))
            .collect::<Result<Vec<_>, _>>()?;
        VariableList::new(bytes).map_err(|err| {
            serde::de::Error::custom(format!("Cannot create VariableList from bytes: {err:?}"))
        })
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconState {
    // Versioning
    #[serde(with = "serde_utils::quoted_u64")]
    pub genesis_time: u64,
    pub genesis_validators_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
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
    #[serde(with = "serde_utils::quoted_u64")]
    pub eth1_deposit_index: u64,

    // Registry
    pub validators: VariableList<Validator, U1099511627776>,
    #[serde(with = "quoted_u64_var_list")]
    pub balances: VariableList<u64, U1099511627776>,

    // Randomness
    pub randao_mixes: FixedVector<B256, U65536>,

    // Slashings
    #[serde(with = "quoted_u64_fixed_vec")]
    pub slashings: FixedVector<u64, U8192>,

    // Participation
    #[serde(with = "quoted_u8_var_list")]
    pub previous_epoch_participation: VariableList<u8, U1099511627776>,
    #[serde(with = "quoted_u8_var_list")]
    pub current_epoch_participation: VariableList<u8, U1099511627776>,

    // Finality
    pub justification_bits: BitVector<U4>,
    pub previous_justified_checkpoint: Checkpoint,
    pub current_justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,

    // Inactivity
    #[serde(with = "quoted_u64_var_list")]
    pub inactivity_scores: VariableList<u64, U1099511627776>,

    // Sync
    pub current_sync_committee: Arc<SyncCommittee>,
    pub next_sync_committee: Arc<SyncCommittee>,

    // Execution
    pub latest_execution_payload_header: ExecutionPayloadHeader,

    // Withdrawals
    #[serde(with = "serde_utils::quoted_u64")]
    pub next_withdrawal_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub next_withdrawal_validator_index: u64,

    // Deep history valid from Capella onwards.
    pub historical_summaries: VariableList<HistoricalSummary, U16777216>,

    // Electra
    #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_requests_start_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub deposit_balance_to_consume: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub exit_balance_to_consume: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub earliest_exit_epoch: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub consolidation_balance_to_consume: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub earliest_consolidation_epoch: u64,
    pub pending_deposits: VariableList<PendingDeposit, U134217728>,
    pub pending_partial_withdrawals: VariableList<PendingPartialWithdrawal, U134217728>,
    pub pending_consolidations: VariableList<PendingConsolidation, U262144>,
}

impl BeaconState {
    /// Return the current epoch.
    pub fn get_current_epoch(&self) -> u64 {
        compute_epoch_at_slot(self.slot)
    }

    pub fn voting_period_start_time(&self) -> u64 {
        let eth1_voting_period_start_slot =
            self.slot - self.slot % (EPOCHS_PER_ETH1_VOTING_PERIOD * SLOTS_PER_EPOCH);
        self.compute_timestamp_at_slot(eth1_voting_period_start_slot)
    }

    pub fn get_eth1_pending_deposit_count(&self) -> u64 {
        let eth1_deposit_index_limit = min(
            self.eth1_data.deposit_count,
            self.deposit_requests_start_index,
        );
        if self.eth1_deposit_index < eth1_deposit_index_limit {
            min(
                MAX_DEPOSITS,
                eth1_deposit_index_limit - self.eth1_deposit_index,
            )
        } else {
            0
        }
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
                if v.is_active_validator(epoch) {
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

            let random_bytes = hash(&[seed.as_slice(), &(i / 16).to_le_bytes()].concat());
            let offset = i % 16 * 2;
            let random_value = bytes_to_int64(&random_bytes[offset..offset + 2]);

            let effective_balance = self.validators[candidate_index as usize].effective_balance;

            if (effective_balance * MAX_RANDOM_VALUE)
                >= (MAX_EFFECTIVE_BALANCE_ELECTRA * random_value as u64)
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
    pub fn get_domain(&self, domain_type: B32, epoch: Option<u64>) -> B256 {
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

    /// Check if ``indexed_attestation`` is not empty, has sorted and unique indices and has a valid
    /// aggregate signature.
    pub fn is_valid_indexed_attestation(
        &self,
        indexed_attestation: &IndexedAttestation,
    ) -> anyhow::Result<bool> {
        let indices: Vec<usize> = indexed_attestation
            .attesting_indices
            .iter()
            .map(|&i| i as usize)
            .collect();
        // Verify indices are sorted and unique
        if indices.is_empty() || !is_sorted_and_unique(&indices) {
            return Ok(false);
        }

        let domain = self.get_domain(
            DOMAIN_BEACON_ATTESTER,
            Some(indexed_attestation.data.target.epoch),
        );
        let signing_root = compute_signing_root(&indexed_attestation.data, domain);

        indexed_attestation
            .signature
            .fast_aggregate_verify(
                indices
                    .iter()
                    .map(|&index| {
                        self.validators
                            .get(index)
                            .map(|validator| &validator.pubkey)
                            .ok_or(anyhow!("Invalid index"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?,
                signing_root.as_ref(),
            )
            .map_err(|e| anyhow!("Invalid indexed attestation: {:?}", e))
    }

    /// Return the set of attesting indices corresponding to ``aggregation_bits`` and
    /// ``committee_bits``.
    pub fn get_attesting_indices(&self, attestation: &Attestation) -> anyhow::Result<HashSet<u64>> {
        let mut output = HashSet::new();
        let mut committee_offset = 0;
        for committee_index in get_committee_indices(&attestation.committee_bits) {
            let committee = self.get_beacon_committee(attestation.data.slot, committee_index)?;

            let mut committee_attesters = HashSet::new();
            for (i, attester_index) in committee.iter().enumerate() {
                if attestation
                    .aggregation_bits
                    .get(committee_offset + i)
                    .map_err(|err| anyhow!("Failed to get aggregation_bit {err:?}"))?
                {
                    committee_attesters.insert(*attester_index);
                }
            }
            output = output.union(&committee_attesters).copied().collect();

            committee_offset += committee.len();
        }
        Ok(output)
    }

    /// Return the indexed attestation corresponding to ``attestation``.
    pub fn get_indexed_attestation(
        &self,
        attestation: &Attestation,
    ) -> anyhow::Result<IndexedAttestation> {
        let attesting_indices = self
            .get_attesting_indices(attestation)?
            .into_iter()
            .sorted()
            .collect::<Vec<_>>();
        Ok(IndexedAttestation {
            attesting_indices: attesting_indices.into(),
            data: attestation.data.clone(),
            signature: attestation.signature.clone(),
        })
    }

    /// Increase the validator balance at index ``index`` by ``delta``.
    pub fn increase_balance(&mut self, index: u64, delta: u64) -> anyhow::Result<()> {
        if let Some(balance) = self.balances.get_mut(index as usize) {
            *balance += delta;
            Ok(())
        } else {
            Err(anyhow!("failed to increase balance"))
        }
    }

    /// Decrease the validator balance at index ``index`` by ``delta`` with underflow protection.
    pub fn decrease_balance(&mut self, index: u64, delta: u64) -> anyhow::Result<()> {
        if let Some(balance) = self.balances.get_mut(index as usize) {
            *balance = balance.saturating_sub(delta);
            Ok(())
        } else {
            Err(anyhow!("failed to decrease balance"))
        }
    }

    /// Initiate the exit of the validator with index ``index``.
    pub fn initiate_validator_exit(&mut self, index: u64) -> anyhow::Result<()> {
        // Return if validator already initiated exit
        let Some(validator) = self.validators.get(index as usize) else {
            bail!("could not get validator")
        };
        if validator.exit_epoch != FAR_FUTURE_EPOCH {
            return Ok(());
        }

        // Compute exit queue epoch
        let exit_queue_epoch =
            self.compute_exit_epoch_and_update_churn(validator.effective_balance);

        let Some(validator) = self.validators.get_mut(index as usize) else {
            bail!("could not get validator")
        };

        // Set validator exit epoch and withdrawable epoch
        validator.exit_epoch = exit_queue_epoch;
        validator.withdrawable_epoch = validator
            .exit_epoch
            .checked_add(MIN_VALIDATOR_WITHDRAWABILITY_DELAY)
            .ok_or(anyhow!("Failed to set withdrawable epoch"))?;

        Ok(())
    }

    /// Slash the validator with index ``slashed_index``
    pub fn slash_validator(
        &mut self,
        slashed_index: u64,
        whistleblower_index: Option<u64>,
    ) -> anyhow::Result<()> {
        let epoch = self.get_current_epoch();

        // Initiate validator exit
        self.initiate_validator_exit(slashed_index)?;

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
            validator_effective_balance / MIN_SLASHING_PENALTY_QUOTIENT_ELECTRA,
        )?;

        // Apply proposer and whistleblower rewards
        let proposer_index = self.get_beacon_proposer_index()?;
        let whistleblower_index = whistleblower_index.unwrap_or(proposer_index);

        let whistleblower_reward =
            validator_effective_balance / WHISTLEBLOWER_REWARD_QUOTIENT_ELECTRA;
        let proposer_reward = whistleblower_reward * PROPOSER_WEIGHT / WEIGHT_DENOMINATOR;
        self.increase_balance(proposer_index, proposer_reward)?;
        self.increase_balance(whistleblower_index, whistleblower_reward - proposer_reward)?;

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

    /// Return a new ``ParticipationFlags`` adding ``flag_index`` to ``flags``.
    pub fn add_flag(flags: u8, flag_index: u8) -> u8 {
        let flag = 1 << flag_index;
        flags | flag
    }

    /// Return whether ``flags`` has ``flag_index`` set.
    pub fn has_flag(flags: u8, flag_index: u8) -> bool {
        let flag = 1 << flag_index;
        flags & flag == flag
    }

    /// Return the set of validator indices that are both active and unslashed for the given
    /// ``flag_index`` and ``epoch``.
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
            .filter(|&index| !self.validators[index as usize].slashed)
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
            / integer_squareroot(self.get_total_active_balance())
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
            if v.is_active_validator(previous_epoch)
                || (v.slashed && previous_epoch + 1 < v.withdrawable_epoch)
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
        data: &AttestationData,
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

        if is_matching_source && inclusion_delay <= integer_squareroot(SLOTS_PER_EPOCH) {
            participation_flag_indices.push(TIMELY_SOURCE_FLAG_INDEX);
        }
        if is_matching_target {
            participation_flag_indices.push(TIMELY_TARGET_FLAG_INDEX);
        }
        if is_matching_head && inclusion_delay == MIN_ATTESTATION_INCLUSION_DELAY {
            participation_flag_indices.push(TIMELY_HEAD_FLAG_INDEX);
        }

        Ok(participation_flag_indices)
    }

    /// Return the inactivity penalty deltas by considering timely target participation flags and
    /// inactivity scores.
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
                    INACTIVITY_SCORE_BIAS * INACTIVITY_PENALTY_QUOTIENT_BELLATRIX;
                penalties[index as usize] += penalty_numerator / penalty_denominator;
            }
        }

        Ok((rewards, penalties))
    }

    pub fn process_block_header(&mut self, block: &BeaconBlock) -> anyhow::Result<()> {
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

    pub fn get_expected_withdrawals(&self) -> anyhow::Result<(Vec<Withdrawal>, u64)> {
        let epoch = self.get_current_epoch();
        let mut withdrawal_index = self.next_withdrawal_index;
        let mut validator_index = self.next_withdrawal_validator_index;
        let mut withdrawals: Vec<Withdrawal> = vec![];
        let mut processed_partial_withdrawals_count = 0;

        // Consume pending partial withdrawals
        for withdrawal in self.pending_partial_withdrawals.iter() {
            if withdrawal.withdrawable_epoch > epoch
                || withdrawals.len() as u64 == MAX_PENDING_PARTIALS_PER_WITHDRAWALS_SWEEP
            {
                break;
            }

            let Some(validator) = self.validators.get(withdrawal.validator_index as usize) else {
                bail!(
                    "Validator index out of bounds: {}",
                    withdrawal.validator_index
                );
            };
            let total_withdrawn = withdrawals
                .iter()
                .filter(|w| w.validator_index == withdrawal.validator_index)
                .map(|w| w.amount)
                .sum::<u64>();
            let balance = *self
                .balances
                .get(withdrawal.validator_index as usize)
                .ok_or(anyhow!(
                    "Balance index out of bounds: {}",
                    withdrawal.validator_index
                ))?
                - total_withdrawn;
            if validator.exit_epoch == FAR_FUTURE_EPOCH
                && validator.effective_balance >= MIN_ACTIVATION_BALANCE
                && balance > MIN_ACTIVATION_BALANCE
            {
                let withdrawable_balance = min(balance - MIN_ACTIVATION_BALANCE, withdrawal.amount);
                withdrawals.push(Withdrawal {
                    index: withdrawal_index,
                    validator_index: withdrawal.validator_index,
                    address: Address::from_slice(&validator.withdrawal_credentials[12..]),
                    amount: withdrawable_balance,
                });
                withdrawal_index += 1;
            }
            processed_partial_withdrawals_count += 1;
        }

        // Sweep for remaining.
        let bound = min(self.validators.len(), MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP);
        for _ in 0..bound {
            let validator = &self
                .validators
                .get(validator_index as usize)
                .ok_or(anyhow!("Validator index out of bounds: {validator_index}"))?;
            let partially_withdrawn_balance = withdrawals
                .iter()
                .filter(|withdrawal| withdrawal.validator_index == validator_index)
                .map(|withdrawal| withdrawal.amount)
                .sum::<u64>();
            let balance = *self
                .balances
                .get(validator_index as usize)
                .ok_or(anyhow!("Balance index out of bounds: {validator_index}"))?
                - partially_withdrawn_balance;
            if validator.is_fully_withdrawable_validator(balance, epoch) {
                withdrawals.push(Withdrawal {
                    index: withdrawal_index,
                    validator_index,
                    address: Address::from_slice(&validator.withdrawal_credentials[12..]),
                    amount: balance,
                });
                withdrawal_index += 1
            } else if validator.is_partially_withdrawable_validator(balance) {
                withdrawals.push(Withdrawal {
                    index: withdrawal_index,
                    validator_index,
                    address: Address::from_slice(&validator.withdrawal_credentials[12..]),
                    amount: balance - validator.get_max_effective_balance(),
                });
                withdrawal_index += 1
            }
            if withdrawals.len() == MAX_WITHDRAWALS_PER_PAYLOAD as usize {
                break;
            }
            validator_index = (validator_index + 1) % self.validators.len() as u64
        }

        Ok((withdrawals, processed_partial_withdrawals_count))
    }

    pub fn process_withdrawals(&mut self, payload: &ExecutionPayload) -> anyhow::Result<()> {
        let (expected_withdrawals, processed_partial_withdrawals_count) =
            self.get_expected_withdrawals()?;
        ensure!(
            payload.withdrawals.deref() == expected_withdrawals,
            "Withdrawals do not match expected withdrawals",
        );

        for withdrawal in &expected_withdrawals {
            self.decrease_balance(withdrawal.validator_index, withdrawal.amount)?;
        }

        let remaining_partial_withdrawals = Vec::from(take(&mut self.pending_partial_withdrawals));
        for partial_withdrawal in remaining_partial_withdrawals
            .into_iter()
            .skip(processed_partial_withdrawals_count as usize)
        {
            self.pending_partial_withdrawals
                .push(partial_withdrawal)
                .map_err(|err| {
                    anyhow!(
                        "Failed to push partial_withdrawal to pending_partial_withdrawals: {err:?}"
                    )
                })?;
        }

        // Update the next withdrawal index if this block contained withdrawals
        if !expected_withdrawals.is_empty() {
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
        self.previous_epoch_participation
            .push(0)
            .map_err(|err| anyhow!("Couldn't push to previous_epoch_participation {:?}", err))?;
        self.current_epoch_participation
            .push(0)
            .map_err(|err| anyhow!("Couldn't push to current_epoch_participation {:?}", err))?;
        self.inactivity_scores
            .push(0)
            .map_err(|err| anyhow!("Couldn't push to inactivity_scores {:?}", err))?;

        Ok(())
    }

    pub fn apply_deposit(
        &mut self,
        pubkey: PubKey,
        withdrawal_credentials: B256,
        amount: u64,
        signature: BLSSignature,
    ) -> anyhow::Result<()> {
        if !self
            .validators
            .iter()
            .any(|validator| validator.pubkey == pubkey)
        {
            // Verify the deposit signature (proof of possession) which is not checked by the
            // deposit contract
            match is_valid_deposit_signature(&pubkey, withdrawal_credentials, amount, &signature) {
                Ok(true) => {
                    self.add_validator_to_registry(pubkey.clone(), withdrawal_credentials, 0)?
                }
                _ => return Ok(()),
            }
        }

        // Increase balance by deposit amount
        self.pending_deposits
            .push(PendingDeposit {
                pubkey,
                withdrawal_credentials,
                amount,
                signature,
                // Use GENESIS_SLOT to distinguish from a pending deposit request
                slot: GENESIS_SLOT,
            })
            .map_err(|err| anyhow!("Couldn't push to pending_deposits {err:?}"))?;

        Ok(())
    }

    pub fn process_deposit(&mut self, deposit: &Deposit) -> anyhow::Result<()> {
        // Verify the Merkle branch
        ensure!(is_valid_merkle_branch(
            deposit.data.tree_hash_root(),
            &deposit.proof,
            // Add 1 for the List length mix-in
            DEPOSIT_CONTRACT_TREE_DEPTH + 1,
            self.eth1_deposit_index,
            self.eth1_data.deposit_root,
        ));

        // Deposits must be processed in order
        self.eth1_deposit_index += 1;

        self.apply_deposit(
            deposit.data.pubkey.clone(),
            deposit.data.withdrawal_credentials,
            deposit.data.amount,
            deposit.data.signature.clone(),
        )
    }

    pub fn process_bls_to_execution_change(
        &mut self,
        signed_address_change: &SignedBLSToExecutionChange,
    ) -> anyhow::Result<()> {
        let address_change = &signed_address_change.message;

        ensure!(address_change.validator_index < self.validators.len() as u64);

        let validator: &Validator = &self.validators[address_change.validator_index as usize];

        ensure!(&validator.withdrawal_credentials[..1] == BLS_WITHDRAWAL_PREFIX);
        ensure!(
            validator.withdrawal_credentials[1..]
                == hash(address_change.from_bls_pubkey.to_bytes())[1..]
        );

        // Fork-agnostic domain since address changes are valid across forks
        let domain = compute_domain(
            DOMAIN_BLS_TO_EXECUTION_CHANGE,
            None,
            Some(self.genesis_validators_root),
        );

        let signing_root = compute_signing_root(address_change, domain);
        ensure!(
            signed_address_change
                .signature
                .verify(&address_change.from_bls_pubkey, signing_root.as_ref())?,
            "BLS Signature verification failed!"
        );

        let withdrawal_credentials = [
            ETH1_ADDRESS_WITHDRAWAL_PREFIX,
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
        signed_voluntary_exit: &SignedVoluntaryExit,
    ) -> anyhow::Result<()> {
        let voluntary_exit = &signed_voluntary_exit.message;
        let validator_index = voluntary_exit.validator_index as usize;

        let validator = self
            .validators
            .get(validator_index)
            .ok_or(anyhow!("Invalid validator index"))?;

        // Verify the validator is active
        ensure!(
            validator.is_active_validator(self.get_current_epoch()),
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

        // Only exit validator if it has no pending withdrawals in the queue
        ensure!(
            self.get_pending_balance_to_withdraw(voluntary_exit.validator_index) == 0,
            "Validator has pending withdrawals"
        );

        // Verify the validator has been active long enough
        let earlist_exit_epoch = validator
            .activation_epoch
            .checked_add(SHARD_COMMITTEE_PERIOD)
            .ok_or(anyhow!("Failed to calculate earliest exit epoch"))?;
        ensure!(
            self.get_current_epoch() >= earlist_exit_epoch,
            "Validator has not been active long enough"
        );

        // Compute signature domain
        let domain = compute_domain(
            DOMAIN_VOLUNTARY_EXIT,
            Some(CAPELLA_FORK_VERSION),
            Some(self.genesis_validators_root),
        );
        let signing_root = compute_signing_root(voluntary_exit, domain);

        ensure!(
            signed_voluntary_exit
                .signature
                .verify(&validator.pubkey, signing_root.as_ref())?,
            "BLS Signature verification failed!"
        );

        // Initiate exit
        self.initiate_validator_exit(validator_index as u64)?;

        Ok(())
    }

    pub fn process_withdrawal_request(
        &mut self,
        withdrawal_request: &WithdrawalRequest,
    ) -> anyhow::Result<()> {
        let amount = withdrawal_request.amount;
        let is_full_exit_request = amount == FULL_EXIT_REQUEST_AMOUNT;

        // If partial withdrawal queue is full, only full exits are processed
        if self.pending_partial_withdrawals.len() as u64 == PENDING_PARTIAL_WITHDRAWALS_LIMIT
            && !is_full_exit_request
        {
            return Ok(());
        }

        // Verify pubkey exists
        let Some((index, validator)) = self
            .validators
            .iter()
            .enumerate()
            .find(|(_, validator)| validator.pubkey == withdrawal_request.validator_pubkey)
        else {
            return Ok(());
        };

        // Verify withdrawal credentials
        let has_correct_credentials = validator.has_execution_withdrawal_credential();
        let is_correct_source_address =
            validator.withdrawal_credentials[12..] == withdrawal_request.source_address;

        if !(has_correct_credentials && is_correct_source_address) {
            return Ok(());
        }

        // Verify the validator is active
        if !validator.is_active_validator(self.get_current_epoch()) {
            return Ok(());
        }

        // Verify exit has not been initiated
        if validator.exit_epoch != FAR_FUTURE_EPOCH {
            return Ok(());
        }

        // Verify the validator has been active long enough
        if self.get_current_epoch() < validator.activation_epoch + SHARD_COMMITTEE_PERIOD {
            return Ok(());
        }

        let pending_balance_to_withdraw = self.get_pending_balance_to_withdraw(index as u64);

        if is_full_exit_request {
            // Only exit validator if it has no pending withdrawals in the queue
            if pending_balance_to_withdraw == 0 {
                self.initiate_validator_exit(index as u64)?;
            }
            return Ok(());
        }

        let has_sufficient_effective_balance =
            validator.effective_balance >= MIN_ACTIVATION_BALANCE;
        let balance = *self
            .balances
            .get(index)
            .ok_or(anyhow!("Failed to get balance"))?;
        let has_excess_balance = balance > MIN_ACTIVATION_BALANCE + pending_balance_to_withdraw;

        // Only allow partial withdrawals with compounding withdrawal credentials
        if validator.has_compounding_withdrawal_credential()
            && has_sufficient_effective_balance
            && has_excess_balance
        {
            let to_withdraw = min(
                balance - MIN_ACTIVATION_BALANCE - pending_balance_to_withdraw,
                amount,
            );
            let exit_queue_epoch = self.compute_exit_epoch_and_update_churn(to_withdraw);
            let withdrawable_epoch = exit_queue_epoch + MIN_VALIDATOR_WITHDRAWABILITY_DELAY;
            self.pending_partial_withdrawals
                .push(PendingPartialWithdrawal {
                    validator_index: index as u64,
                    amount: to_withdraw,
                    withdrawable_epoch,
                }).map_err(|err| anyhow!("Failed to push PendingPartialWithdrawal to pending_partial_withdrawals {err:?}"))?;
        }

        Ok(())
    }

    pub fn process_deposit_request(
        &mut self,
        deposit_request: &DepositRequest,
    ) -> anyhow::Result<()> {
        // Set deposit request start index
        if self.deposit_requests_start_index == UNSET_DEPOSIT_REQUESTS_START_INDEX {
            self.deposit_requests_start_index = deposit_request.index;
        }

        // Create pending deposit
        self.pending_deposits
            .push(PendingDeposit {
                pubkey: deposit_request.pubkey.clone(),
                withdrawal_credentials: deposit_request.withdrawal_credentials,
                amount: deposit_request.amount,
                signature: deposit_request.signature.clone(),
                slot: self.slot,
            })
            .map_err(|err| anyhow!("Failed to push PendingDeposit to pending_deposits {err:?}"))?;

        Ok(())
    }

    pub fn is_valid_switch_to_compounding_request(
        &self,
        consolidation_request: &ConsolidationRequest,
    ) -> bool {
        // Switch to compounding requires source and target be equal
        if consolidation_request.source_pubkey != consolidation_request.target_pubkey {
            return false;
        }

        // Verify pubkey exists
        let Some(source_validator) = self
            .validators
            .iter()
            .find(|validator| validator.pubkey == consolidation_request.source_pubkey)
        else {
            return false;
        };

        // Verify request has been authorized
        if source_validator.withdrawal_credentials[12..] != consolidation_request.source_address {
            return false;
        }

        // Verify source withdrawal credentials
        if !source_validator.has_eth1_withdrawal_credential() {
            return false;
        }

        // Verify the source is active
        if !source_validator.is_active_validator(self.get_current_epoch()) {
            return false;
        }

        // Verify exit for source has not been initiated
        if source_validator.exit_epoch != FAR_FUTURE_EPOCH {
            return false;
        }

        true
    }

    pub fn process_consolidation_request(
        &mut self,
        consolidation_request: &ConsolidationRequest,
    ) -> anyhow::Result<()> {
        if self.is_valid_switch_to_compounding_request(consolidation_request) {
            let Some((index, _)) = self
                .validators
                .iter()
                .enumerate()
                .find(|(_, validator)| validator.pubkey == consolidation_request.source_pubkey)
            else {
                bail!("Validator not found");
            };
            self.switch_to_compounding_validator(index as u64)?;
            return Ok(());
        }

        // Verify that source != target, so a consolidation cannot be used as an exit
        if consolidation_request.source_pubkey == consolidation_request.target_pubkey {
            return Ok(());
        }

        // If the pending consolidations queue is full, consolidation requests are ignored
        if self.pending_consolidations.len() as u64 == PENDING_CONSOLIDATIONS_LIMIT {
            return Ok(());
        }

        // If there is too little available consolidation churn limit, consolidation requests are
        // ignored
        if self.get_consolidation_churn_limit() <= MIN_ACTIVATION_BALANCE {
            return Ok(());
        }

        let Some((source_index, source_validator)) = self
            .validators
            .iter()
            .enumerate()
            .find(|(_, validator)| validator.pubkey == consolidation_request.source_pubkey)
        else {
            return Ok(());
        };
        let Some((target_index, target_validator)) = self
            .validators
            .iter()
            .enumerate()
            .find(|(_, validator)| validator.pubkey == consolidation_request.target_pubkey)
        else {
            return Ok(());
        };

        // Verify source withdrawal credentials
        let has_correct_credential = source_validator.has_execution_withdrawal_credential();
        let is_correct_source_address =
            source_validator.withdrawal_credentials[12..] == consolidation_request.source_address;
        if !(has_correct_credential && is_correct_source_address) {
            return Ok(());
        }

        // Verify that target has compounding withdrawal credentials
        if !target_validator.has_compounding_withdrawal_credential() {
            return Ok(());
        }

        // Verify the source and the target are active
        let current_epoch = self.get_current_epoch();
        if !source_validator.is_active_validator(current_epoch)
            || !target_validator.is_active_validator(current_epoch)
        {
            return Ok(());
        }

        // Verify exits for source and target have not been initiated
        if source_validator.exit_epoch != FAR_FUTURE_EPOCH
            || target_validator.exit_epoch != FAR_FUTURE_EPOCH
        {
            return Ok(());
        }

        // Verify the source has been active long enough
        if current_epoch < source_validator.activation_epoch + SHARD_COMMITTEE_PERIOD {
            return Ok(());
        }

        // Verify the source has no pending withdrawals in the queue
        if self.get_pending_balance_to_withdraw(source_index as u64) > 0 {
            return Ok(());
        }

        // Initiate source validator exit and append pending consolidation
        let exit_epoch =
            self.compute_consolidation_epoch_and_update_churn(source_validator.effective_balance);
        let Some(source_validator) = self.validators.get_mut(source_index) else {
            bail!("Validator not found");
        };
        source_validator.exit_epoch = exit_epoch;
        source_validator.withdrawable_epoch =
            source_validator.exit_epoch + MIN_VALIDATOR_WITHDRAWABILITY_DELAY;

        self.pending_consolidations
            .push(PendingConsolidation {
                source_index: source_index as u64,
                target_index: target_index as u64,
            })
            .map_err(|err| {
                anyhow!("Failed to push PendingConsolidation to pending_consolidations {err:?}")
            })?;

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

            let random_bytes = hash(&[seed.as_slice(), &(i / 16).to_le_bytes()].concat());
            let offset = i % 16 * 2;
            let random_value = bytes_to_int64(&random_bytes[offset..offset + 2]);
            let effective_balance = self.validators[candidate_index as usize].effective_balance;
            if effective_balance * MAX_RANDOM_VALUE
                >= MAX_EFFECTIVE_BALANCE_ELECTRA * random_value as u64
            {
                sync_committee_indices.push(candidate_index)
            }
            i += 1
        }

        Ok(sync_committee_indices)
    }

    pub fn process_proposer_slashing(
        &mut self,
        proposer_slashing: &ProposerSlashing,
    ) -> anyhow::Result<()> {
        let header_1 = &proposer_slashing.signed_header_1.message;
        let header_2 = &proposer_slashing.signed_header_2.message;

        // Verify header slots match
        ensure!(header_1.slot == header_2.slot, "Header slots must match");

        // Verify header proposer indices match
        ensure!(
            header_1.proposer_index == header_2.proposer_index,
            "Proposer indices must match"
        );

        // Verify the headers are different
        ensure!(header_1 != header_2, "Headers must be different");

        // Get the proposer and verify they are slashable
        let proposer_index = header_1.proposer_index;
        let proposer = self
            .validators
            .get(proposer_index as usize)
            .ok_or_else(|| anyhow::anyhow!("Invalid proposer index"))?;

        ensure!(
            proposer.is_slashable_validator(self.get_current_epoch()),
            "Proposer is not slashable"
        );

        // Verify signatures
        for signed_header in [
            &proposer_slashing.signed_header_1,
            &proposer_slashing.signed_header_2,
        ] {
            let domain = self.get_domain(
                DOMAIN_BEACON_PROPOSER,
                Some(compute_epoch_at_slot(signed_header.message.slot)),
            );

            let signing_root = compute_signing_root(&signed_header.message, domain);

            ensure!(
                signed_header
                    .signature
                    .verify(&proposer.pubkey, signing_root.as_ref())?,
                "BLS Signature verification failed!"
            );
        }

        // Slash the validator
        self.slash_validator(proposer_index, None)
    }

    pub fn process_historical_summaries_update(&mut self) -> anyhow::Result<()> {
        // Set historical block root accumulator.
        let next_epoch = self.get_current_epoch() + 1;
        if next_epoch % (SLOTS_PER_HISTORICAL_ROOT / SLOTS_PER_EPOCH) == 0 {
            let historical_summary = HistoricalSummary {
                block_summary_root: self.block_roots.tree_hash_root(),
                state_summary_root: self.state_roots.tree_hash_root(),
            };
            self.historical_summaries
                .push(historical_summary)
                .map_err(|err| anyhow!("Failed to push historical summary: {err:?}"))?;
        }

        Ok(())
    }

    pub fn process_attester_slashing(
        &mut self,
        attester_slashing: &AttesterSlashing,
    ) -> anyhow::Result<()> {
        let attestation_1 = &attester_slashing.attestation_1;
        let attestation_2 = &attester_slashing.attestation_2;

        // Ensure the two attestations are slashable
        ensure!(
            is_slashable_attestation_data(&attestation_1.data, &attestation_2.data),
            "Attestations are not slashable"
        );

        // Validate both attestations
        ensure!(
            self.is_valid_indexed_attestation(attestation_1)?,
            "First attestation is invalid"
        );
        ensure!(
            self.is_valid_indexed_attestation(attestation_2)?,
            "Second attestation is invalid"
        );

        let current_epoch = self.get_current_epoch();
        let indices_1: HashSet<_> = attestation_1.attesting_indices.iter().cloned().collect();
        let indices_2: HashSet<_> = attestation_2.attesting_indices.iter().cloned().collect();

        let mut slashed_any = false;

        // Find common attesting indices and process slashing
        for &index in indices_1.intersection(&indices_2).sorted() {
            if self.validators[index as usize].is_slashable_validator(current_epoch) {
                self.slash_validator(index, None)?;
                slashed_any = true;
            }
        }

        ensure!(slashed_any, "No validator was slashed");

        Ok(())
    }

    pub fn process_sync_aggregate(&mut self, sync_aggregate: &SyncAggregate) -> anyhow::Result<()> {
        // Verify sync committee aggregate signature signing over the previous slot block root
        let committee_pubkeys = &self.current_sync_committee.pubkeys;
        let mut participant_pubkeys = vec![];

        for (pubkey, bit) in committee_pubkeys
            .iter()
            .zip(sync_aggregate.sync_committee_bits.iter())
        {
            if bit {
                participant_pubkeys.push(pubkey);
            }
        }

        let previous_slot = max(self.slot, 1) - 1;
        let domain = self.get_domain(
            DOMAIN_SYNC_COMMITTEE,
            Some(compute_epoch_at_slot(previous_slot)),
        );
        let signing_root =
            compute_signing_root(self.get_block_root_at_slot(previous_slot)?, domain);

        ensure!(
            eth_fast_aggregate_verify(
                &participant_pubkeys,
                signing_root,
                &sync_aggregate.sync_committee_signature,
            )?,
            "Sync aggregate signature verification failed."
        );

        // Compute participant and proposer rewards
        let total_active_increments = self.get_total_active_balance() / EFFECTIVE_BALANCE_INCREMENT;
        let total_base_rewards = self.get_base_reward_per_increment() * total_active_increments;
        let max_participant_rewards =
            total_base_rewards * SYNC_REWARD_WEIGHT / WEIGHT_DENOMINATOR / SLOTS_PER_EPOCH;
        let participant_reward = max_participant_rewards / SYNC_COMMITTEE_SIZE;
        let proposer_reward =
            participant_reward * PROPOSER_WEIGHT / (WEIGHT_DENOMINATOR - PROPOSER_WEIGHT);

        // Apply participant and proposer rewards
        let mut all_pubkeys = vec![];
        for validator in &self.validators {
            all_pubkeys.push(validator.pubkey.clone());
        }

        let mut committee_indices = vec![];
        for pubkey in &self.current_sync_committee.pubkeys {
            let index = all_pubkeys
                .iter()
                .position(|r| r == pubkey)
                .ok_or_else(|| anyhow!("Pubkey not found in all_pubkeys."))?;
            committee_indices.push(index);
        }

        for (participant_index, participation_bit) in committee_indices
            .iter()
            .zip(sync_aggregate.sync_committee_bits.iter())
        {
            if participation_bit {
                self.increase_balance(*participant_index as u64, participant_reward)?;
                self.increase_balance(self.get_beacon_proposer_index()?, proposer_reward)?;
            } else {
                self.decrease_balance(*participant_index as u64, participant_reward)?;
            }
        }

        Ok(())
    }

    pub fn process_justification_and_finalization(&mut self) -> anyhow::Result<()> {
        // Initial FFG checkpoint values have a `0x00` stub for `root`.
        // Skip FFG updates in the first two epochs to avoid corner cases that might result in
        // modifying this stub.
        if self.get_current_epoch() <= GENESIS_EPOCH + 1 {
            return Ok(());
        }

        let previous_indices = self.get_unslashed_participating_indices(
            TIMELY_TARGET_FLAG_INDEX,
            self.get_previous_epoch(),
        )?;
        let current_indices = self.get_unslashed_participating_indices(
            TIMELY_TARGET_FLAG_INDEX,
            self.get_current_epoch(),
        )?;

        let total_active_balance = self.get_total_active_balance();
        let previous_target_balance = self.get_total_balance(previous_indices);
        let current_target_balance = self.get_total_balance(current_indices);

        self.weigh_justification_and_finalization(
            total_active_balance,
            previous_target_balance,
            current_target_balance,
        )?;

        Ok(())
    }

    pub fn weigh_justification_and_finalization(
        &mut self,
        total_active_balance: u64,
        previous_epoch_target_balance: u64,
        current_epoch_target_balance: u64,
    ) -> anyhow::Result<()> {
        let previous_epoch = self.get_previous_epoch();
        let current_epoch = self.get_current_epoch();
        let old_previous_justified_checkpoint = self.previous_justified_checkpoint;
        let old_current_justified_checkpoint = self.current_justified_checkpoint;

        self.previous_justified_checkpoint = self.current_justified_checkpoint;

        for i in (1..JUSTIFICATION_BITS_LENGTH).rev() {
            let bit = self
                .justification_bits
                .get(i - 1)
                .map_err(|err| anyhow!("Failed to get justification bit {err:?}"))?;
            self.justification_bits
                .set(i, bit)
                .map_err(|err| anyhow!("Failed to set justification bit {err:?}"))?;
        }

        self.justification_bits
            .set(0, false)
            .map_err(|err| anyhow!("Failed to set justification bit 0: {err:?}"))?;

        if previous_epoch_target_balance * 3 >= total_active_balance * 2 {
            self.current_justified_checkpoint = Checkpoint {
                epoch: previous_epoch,
                root: self.get_block_root(previous_epoch)?,
            };
            self.justification_bits
                .set(1, true)
                .map_err(|err| anyhow!("Failed to set justification bit 1: {err:?}"))?;
        }

        if current_epoch_target_balance * 3 >= total_active_balance * 2 {
            self.current_justified_checkpoint = Checkpoint {
                epoch: current_epoch,
                root: self.get_block_root(current_epoch)?,
            };
            self.justification_bits
                .set(0, true)
                .map_err(|err| anyhow!("Failed to set justification bit 0: {err:?}"))?;
        }

        // Process finalizations
        let bits: Vec<bool> = self.justification_bits.iter().collect();

        // The 2nd/3rd/4th most recent epochs are justified, the 2nd using the 4th as source
        if bits[1..4].iter().all(|&b| b)
            && old_previous_justified_checkpoint.epoch + 3 == current_epoch
        {
            self.finalized_checkpoint = old_previous_justified_checkpoint;
        }

        // The 2nd/3rd most recent epochs are justified, the 2nd using the 3rd as source
        if bits[1..3].iter().all(|&b| b)
            && old_previous_justified_checkpoint.epoch + 2 == current_epoch
        {
            self.finalized_checkpoint = old_previous_justified_checkpoint;
        }

        // The 1st/2nd/3rd most recent epochs are justified, the 1st using the 3rd as source
        if bits[0..3].iter().all(|&b| b)
            && old_current_justified_checkpoint.epoch + 2 == current_epoch
        {
            self.finalized_checkpoint = old_current_justified_checkpoint;
        }

        // The 1st/2nd most recent epochs are justified, the 1st using the 2nd as source
        if bits[0..2].iter().all(|&b| b)
            && old_current_justified_checkpoint.epoch + 1 == current_epoch
        {
            self.finalized_checkpoint = old_current_justified_checkpoint;
        }

        Ok(())
    }

    pub fn process_eth1_data_reset(&mut self) -> anyhow::Result<()> {
        let next_epoch = self.get_current_epoch() + 1;

        // Reset eth1 data votes
        if next_epoch % EPOCHS_PER_ETH1_VOTING_PERIOD == 0 {
            self.eth1_data_votes = VariableList::default();
        }

        Ok(())
    }

    pub fn process_effective_balance_updates(&mut self) -> anyhow::Result<()> {
        // Update effective balances with hysteresis
        for (index, validator) in self.validators.iter_mut().enumerate() {
            let balance = self.balances[index];
            let hysteresis_increment = EFFECTIVE_BALANCE_INCREMENT / HYSTERESIS_QUOTIENT;
            let downward_threshold = hysteresis_increment * HYSTERESIS_DOWNWARD_MULTIPLIER;
            let upward_threshold = hysteresis_increment * HYSTERESIS_UPWARD_MULTIPLIER;

            if balance + downward_threshold < validator.effective_balance
                || validator.effective_balance + upward_threshold < balance
            {
                validator.effective_balance = (balance - balance % EFFECTIVE_BALANCE_INCREMENT)
                    .min(validator.get_max_effective_balance());
            }
        }

        Ok(())
    }

    pub fn process_randao(&mut self, body: &BeaconBlockBody) -> anyhow::Result<()> {
        let epoch = self.get_current_epoch();

        // Verify RANDAO reveal
        if let Some(proposer) = self
            .validators
            .get(self.get_beacon_proposer_index()? as usize)
        {
            let signing_root =
                compute_signing_root(epoch, self.get_domain(DOMAIN_RANDAO, Some(epoch)));
            ensure!(
                body.randao_reveal
                    .verify(&proposer.pubkey, signing_root.as_ref())?,
                "BLS Signature verification failed!"
            );

            // Mix in RANDAO reveal
            let mix = xor(
                self.get_randao_mix(epoch).as_slice(),
                hash(body.randao_reveal.to_bytes()).as_slice(),
            );
            self.randao_mixes[(epoch % EPOCHS_PER_HISTORICAL_VECTOR) as usize] = mix;
        }

        Ok(())
    }

    pub fn process_eth1_data(&mut self, body: &BeaconBlockBody) -> anyhow::Result<()> {
        self.eth1_data_votes
            .push(body.eth1_data.clone())
            .map_err(|err| anyhow!("Can't push eth1_data {err:?}"))?;

        let count = self
            .eth1_data_votes
            .iter()
            .filter(|data| **data == body.eth1_data)
            .count() as u64;

        if count * 2 > (EPOCHS_PER_ETH1_VOTING_PERIOD * SLOTS_PER_EPOCH) {
            self.eth1_data = body.eth1_data.clone();
        }

        Ok(())
    }

    pub fn process_attestation(&mut self, attestation: &Attestation) -> anyhow::Result<()> {
        let data = &attestation.data;
        ensure!(
            data.target.epoch == self.get_previous_epoch()
                || data.target.epoch == self.get_current_epoch(),
            "Target epoch must be the previous or current epoch"
        );

        ensure!(
            data.target.epoch == compute_epoch_at_slot(data.slot),
            "Target epoch must match the computed epoch at slot"
        );

        ensure!(
            data.slot + MIN_ATTESTATION_INCLUSION_DELAY <= self.slot,
            "Attestation must be included after the minimum delay"
        );

        ensure!(
            data.index < self.get_committee_count_per_slot(data.target.epoch),
            "Committee index must be within bounds"
        );

        ensure!(data.index == 0);
        let committee_indices = get_committee_indices(&attestation.committee_bits);
        let mut committee_offset = 0;
        for committee_index in committee_indices {
            ensure!(committee_index < self.get_committee_count_per_slot(data.target.epoch));
            let committee = self.get_beacon_committee(data.slot, data.index)?;
            let mut committee_attesters = HashSet::new();
            for (i, &attester_index) in committee.iter().enumerate() {
                if attestation
                    .aggregation_bits
                    .get(committee_offset + i)
                    .map_err(|err| anyhow!("Failed to get aggregation bit {err:?}"))?
                {
                    committee_attesters.insert(attester_index);
                }
            }
            ensure!(
                !committee_attesters.is_empty(),
                "Committee attesters must not be empty"
            );
            committee_offset += committee.len();
        }

        // Bitfield length matches total number of participants
        ensure!(
            attestation.aggregation_bits.len() == committee_offset,
            "Aggregation bits length must match committee size"
        );

        // Participation flag indices
        let participation_flag_indices =
            self.get_attestation_participation_flag_indices(data, self.slot - data.slot)?;

        // Verify signature
        ensure!(
            self.is_valid_indexed_attestation(&self.get_indexed_attestation(attestation)?)?,
            "Attestation signature must be valid"
        );

        let attesting_indices = self.get_attesting_indices(attestation)?;
        let base_rewards: Vec<_> = attesting_indices
            .iter()
            .map(|&index| (index, self.get_base_reward(index)))
            .collect();

        // Update epoch participation flags
        let epoch_participation = if data.target.epoch == self.get_current_epoch() {
            &mut self.current_epoch_participation
        } else {
            &mut self.previous_epoch_participation
        };

        let mut proposer_reward_numerator = 0;

        for (index, base_reward) in base_rewards {
            for (flag_index, &weight) in PARTICIPATION_FLAG_WEIGHTS.iter().enumerate() {
                let flag_index = flag_index as u8;

                if participation_flag_indices.contains(&flag_index) {
                    let epoch_part =
                        epoch_participation.get_mut(index as usize).ok_or_else(|| {
                            anyhow!("Index {} out of bounds in epoch_participation", index)
                        })?;

                    if !Self::has_flag(*epoch_part, flag_index) {
                        *epoch_part = Self::add_flag(*epoch_part, flag_index);
                        proposer_reward_numerator += base_reward * weight;
                    }
                }
            }
        }

        // Reward proposer
        let proposer_reward_denominator =
            (WEIGHT_DENOMINATOR - PROPOSER_WEIGHT) * WEIGHT_DENOMINATOR / PROPOSER_WEIGHT;
        let proposer_reward = proposer_reward_numerator / proposer_reward_denominator;
        self.increase_balance(self.get_beacon_proposer_index()?, proposer_reward)?;

        Ok(())
    }

    pub fn process_randao_mixes_reset(&mut self) -> anyhow::Result<()> {
        let current_epoch = self.get_current_epoch();
        let next_epoch = current_epoch + 1;
        // Set randao mix
        self.randao_mixes[(next_epoch % EPOCHS_PER_HISTORICAL_VECTOR) as usize] =
            self.get_randao_mix(current_epoch);

        Ok(())
    }

    pub fn process_slashings_reset(&mut self) -> anyhow::Result<()> {
        let next_epoch = self.get_current_epoch() + 1;
        // Reset slashings
        self.slashings[(next_epoch % EPOCHS_PER_SLASHINGS_VECTOR) as usize] = 0;

        Ok(())
    }

    pub fn process_slashings(&mut self) -> anyhow::Result<()> {
        let epoch = self.get_current_epoch();
        let total_balance = self.get_total_active_balance();
        let adjusted_total_slashing_balance = (self.slashings.iter().sum::<u64>()
            * PROPORTIONAL_SLASHING_MULTIPLIER_BELLATRIX)
            .min(total_balance);

        // Factored out from penalty numerator to avoid uint64 overflow
        let increment = EFFECTIVE_BALANCE_INCREMENT;
        let penalty_per_effective_balance_increment =
            adjusted_total_slashing_balance / (total_balance / increment);
        for index in 0..self.validators.len() {
            let validator = &self
                .validators
                .get(index)
                .ok_or_else(|| anyhow!("Invalid validator index: {index}"))?;
            if validator.slashed
                && epoch + EPOCHS_PER_SLASHINGS_VECTOR / 2 == validator.withdrawable_epoch
            {
                let effective_balance_increments = validator.effective_balance / increment;
                let penalty =
                    penalty_per_effective_balance_increment * effective_balance_increments;

                self.decrease_balance(index as u64, penalty)?;
            }
        }

        Ok(())
    }

    /// Applies ``deposit`` to the ``state``.
    pub fn apply_pending_deposit(&mut self, deposit: &PendingDeposit) -> anyhow::Result<()> {
        if let Some((index, _validator)) = self
            .validators
            .iter()
            .enumerate()
            .find(|(_, v)| v.pubkey == deposit.pubkey)
        {
            self.increase_balance(index as u64, deposit.amount)?;
        } else {
            // Verify the deposit signature (proof of possession) which is not checked by the
            // deposit contract
            if is_valid_deposit_signature(
                &deposit.pubkey,
                deposit.withdrawal_credentials,
                deposit.amount,
                &deposit.signature,
            )
            .unwrap_or_default()
            {
                self.add_validator_to_registry(
                    deposit.pubkey.clone(),
                    deposit.withdrawal_credentials,
                    deposit.amount,
                )?;
            }
        }

        Ok(())
    }

    pub fn process_pending_deposits(&mut self) -> anyhow::Result<()> {
        let next_epoch = self.get_current_epoch() + 1;
        let available_for_processing =
            self.deposit_balance_to_consume + self.get_activation_exit_churn_limit();
        let mut processed_amount = 0;
        let mut next_deposit_index = 0;
        let mut deposits_to_postpone = vec![];
        let mut is_churn_limit_reached = false;
        let finalized_slot = compute_start_slot_at_epoch(self.finalized_checkpoint.epoch);

        for index in 0..self.pending_deposits.len() {
            let Some(deposit) = self.pending_deposits.get(index).cloned() else {
                bail!("Pending deposit not found");
            };
            // Do not process deposit requests if Eth1 bridge deposits are not yet applied
            if deposit.slot > GENESIS_SLOT
                && self.eth1_deposit_index < self.deposit_requests_start_index
            {
                break;
            }

            // Check if deposit has been finalized, otherwise, stop processing.
            if deposit.slot > finalized_slot {
                break;
            }

            // Check if number of processed deposits has not reached the limit, otherwise, stop
            // processing.
            if next_deposit_index >= MAX_PENDING_DEPOSITS_PER_EPOCH {
                break;
            }

            // Read validator state
            let (is_validator_exited, is_validator_withdrawn) = if let Some(validator) = self
                .validators
                .iter()
                .find(|validator| validator.pubkey == deposit.pubkey)
            {
                (
                    validator.exit_epoch < FAR_FUTURE_EPOCH,
                    validator.withdrawable_epoch < next_epoch,
                )
            } else {
                (false, false)
            };

            if is_validator_withdrawn {
                // Deposited balance will never become active. Increase balance but do not consume
                // churn
                self.apply_pending_deposit(&deposit)?;
            } else if is_validator_exited {
                // Validator is exiting, postpone the deposit until after withdrawable epoch
                deposits_to_postpone.push(deposit.clone());
            } else {
                // Check if deposit fits in the churn, otherwise, do no more deposit processing in
                // this epoch.
                is_churn_limit_reached =
                    processed_amount + deposit.amount > available_for_processing;
                if is_churn_limit_reached {
                    break;
                }

                // Consume churn and apply deposit.
                processed_amount += deposit.amount;
                self.apply_pending_deposit(&deposit)?;
            }

            // Regardless of how the deposit was handled, we move on in the queue.
            next_deposit_index += 1;
        }

        let remaining_deposits = Vec::from(take(&mut self.pending_deposits));
        for deposit in remaining_deposits
            .into_iter()
            .skip(next_deposit_index as usize)
            .chain(deposits_to_postpone)
        {
            self.pending_deposits
                .push(deposit)
                .map_err(|err| anyhow!("Failed to push deposit to pending deposits: {err:?}"))?;
        }

        // Accumulate churn only if the churn limit has been hit.
        self.deposit_balance_to_consume = if is_churn_limit_reached {
            available_for_processing - processed_amount
        } else {
            0
        };

        Ok(())
    }

    pub fn process_pending_consolidations(&mut self) -> anyhow::Result<()> {
        let next_epoch = self.get_current_epoch() + 1;
        let mut next_pending_consolidation = 0;
        for index in 0..self.pending_consolidations.len() {
            let Some(pending_consolidation) = self.pending_consolidations.get(index).cloned()
            else {
                bail!("Pending consolidation not found");
            };
            let Some(source_validator) = self
                .validators
                .get(pending_consolidation.source_index as usize)
            else {
                return Err(anyhow!("Validator not found"));
            };

            if source_validator.slashed {
                next_pending_consolidation += 1;
                continue;
            }
            if source_validator.withdrawable_epoch > next_epoch {
                break;
            }

            // Calculate the consolidated balance
            let source_effective_balance = min(
                *self
                    .balances
                    .get(pending_consolidation.source_index as usize)
                    .ok_or(anyhow!("Failed to get balance"))?,
                source_validator.effective_balance,
            );

            // Move active balance to target. Excess balance is withdrawable.
            self.decrease_balance(pending_consolidation.source_index, source_effective_balance)?;
            self.increase_balance(pending_consolidation.target_index, source_effective_balance)?;
            next_pending_consolidation += 1;
        }

        let remaining_consolidations = Vec::from(take(&mut self.pending_consolidations));
        for pending_consolidation in remaining_consolidations
            .into_iter()
            .skip(next_pending_consolidation)
        {
            self.pending_consolidations
                .push(pending_consolidation)
                .map_err(|err| anyhow!("Failed to push pending_consolidation: {err:?}"))?;
        }

        Ok(())
    }

    pub fn process_operations(&mut self, body: &BeaconBlockBody) -> anyhow::Result<()> {
        // Disable former deposit mechanism once all prior deposits are processed
        let eth1_deposit_index_limit = min(
            self.eth1_data.deposit_count,
            self.deposit_requests_start_index,
        );
        if self.eth1_deposit_index < eth1_deposit_index_limit {
            ensure!(
                body.deposits.len() as u64
                    == MAX_DEPOSITS.min(eth1_deposit_index_limit - self.eth1_deposit_index,),
            );
        } else {
            ensure!(body.deposits.is_empty());
        }

        for proposer_slashing in body.proposer_slashings.iter() {
            self.process_proposer_slashing(proposer_slashing)?;
        }
        for attester_slashing in body.attester_slashings.iter() {
            self.process_attester_slashing(attester_slashing)?;
        }
        for attestation in body.attestations.iter() {
            self.process_attestation(attestation)?;
        }
        for deposit in body.deposits.iter() {
            self.process_deposit(deposit)?;
        }
        for voluntary_exit in body.voluntary_exits.iter() {
            self.process_voluntary_exit(voluntary_exit)?;
        }
        for bls_to_execution_change in body.bls_to_execution_changes.iter() {
            self.process_bls_to_execution_change(bls_to_execution_change)?;
        }
        for deposit in body.execution_requests.deposits.iter() {
            self.process_deposit_request(deposit)?;
        }
        for withdrawal in body.execution_requests.withdrawals.iter() {
            self.process_withdrawal_request(withdrawal)?;
        }
        for consolidation in body.execution_requests.consolidations.iter() {
            self.process_consolidation_request(consolidation)?;
        }

        Ok(())
    }

    pub fn verify_block_signature(&self, signed_block: &SignedBeaconBlock) -> anyhow::Result<bool> {
        let proposer = &self.validators[signed_block.message.proposer_index as usize];
        let signing_root = compute_signing_root(
            signed_block.message.clone(),
            self.get_domain(DOMAIN_BEACON_PROPOSER, None),
        );

        signed_block
            .signature
            .verify(&proposer.pubkey, signing_root.as_ref())
            .map_err(|e| anyhow!("Invalid block signature: {:?}", e))
    }

    /// Check if ``validator`` is eligible for activation.
    pub fn is_eligible_for_activation(
        finalized_checkpoint_epoch: u64,
        validator: &Validator,
    ) -> bool {
        // Placement in queue is finalized
        validator.activation_eligibility_epoch <= finalized_checkpoint_epoch
            && validator.activation_epoch == FAR_FUTURE_EPOCH
    }

    /// Return the validator activation churn limit for the current epoch.
    pub fn get_validator_activation_churn_limit(&self) -> u64 {
        min(
            MAX_PER_EPOCH_ACTIVATION_CHURN_LIMIT,
            self.get_validator_churn_limit(),
        )
    }

    pub fn process_registry_updates(&mut self) -> anyhow::Result<()> {
        let current_epoch = self.get_current_epoch();
        let activation_epoch = compute_activation_exit_epoch(current_epoch);

        // Process activation eligibility, ejections, and activations
        let mut initiate_validator = vec![];
        let finalized_checkpoint_epoch = self.finalized_checkpoint.epoch;
        for (index, validator) in self.validators.iter_mut().enumerate() {
            if validator.is_eligible_for_activation_queue() {
                validator.activation_eligibility_epoch =
                    current_epoch.checked_add(1).ok_or_else(|| {
                        anyhow::anyhow!("Epoch overflow when setting activation eligibility epoch")
                    })?;
            } else if validator.is_active_validator(current_epoch)
                && validator.effective_balance <= EJECTION_BALANCE
            {
                initiate_validator.push(index as u64);
            } else if Self::is_eligible_for_activation(finalized_checkpoint_epoch, validator) {
                validator.activation_epoch = activation_epoch;
            }
        }

        for index in initiate_validator {
            self.initiate_validator_exit(index)?;
        }

        Ok(())
    }

    /// Return the deltas for a given ``flag_index`` by scanning through the participation flags.
    pub fn get_flag_index_deltas(&self, flag_index: u8) -> anyhow::Result<(Vec<u64>, Vec<u64>)> {
        let mut rewards = vec![0; self.validators.len()];
        let mut penalties = vec![0; self.validators.len()];

        let previous_epoch = self.get_previous_epoch();
        let unslashed_participating_indices =
            self.get_unslashed_participating_indices(flag_index, previous_epoch)?;
        let weight = PARTICIPATION_FLAG_WEIGHTS[flag_index as usize];
        let unslashed_participating_balance =
            self.get_total_balance(unslashed_participating_indices.clone());
        let unslashed_participating_increments =
            unslashed_participating_balance / EFFECTIVE_BALANCE_INCREMENT;
        let active_increments = self.get_total_active_balance() / EFFECTIVE_BALANCE_INCREMENT;

        for index in self.get_eligible_validator_indices()? {
            let base_reward = self.get_base_reward(index);

            if unslashed_participating_indices.contains(&index) {
                if !self.is_in_inactivity_leak() {
                    let reward_numerator =
                        base_reward * weight * unslashed_participating_increments;
                    rewards[index as usize] +=
                        reward_numerator / (active_increments * WEIGHT_DENOMINATOR);
                }
            } else if flag_index != TIMELY_HEAD_FLAG_INDEX {
                penalties[index as usize] += base_reward * weight / WEIGHT_DENOMINATOR;
            }
        }

        Ok((rewards, penalties))
    }

    pub fn process_rewards_and_penalties(&mut self) -> anyhow::Result<()> {
        // No rewards are applied at the end of `GENESIS_EPOCH` because rewards are for work done in
        // the previous epoch
        if self.get_current_epoch() == GENESIS_EPOCH {
            return Ok(());
        }

        // Get deltas for each flag index and inactivity penalties
        let mut deltas = vec![];

        // Collect the flag deltas for each participation flag index
        for flag_index in 0..PARTICIPATION_FLAG_WEIGHTS.len() {
            deltas.push(self.get_flag_index_deltas(flag_index as u8)?);
        }

        // Add the inactivity penalties
        deltas.push(self.get_inactivity_penalty_deltas()?);

        // Iterate over rewards and penalties for each delta
        for (rewards, penalties) in deltas {
            for index in 0..self.validators.len() {
                self.increase_balance(index as u64, rewards[index])?;
                self.decrease_balance(index as u64, penalties[index])?;
            }
        }

        Ok(())
    }

    /// Return the next sync committee, with possible pubkey duplicates.
    pub fn get_next_sync_committee(&self) -> anyhow::Result<SyncCommittee> {
        let indices = self.get_next_sync_committee_indices()?;
        let mut pubkeys = vec![];

        for index in indices {
            pubkeys.push(self.validators[index as usize].pubkey.clone());
        }

        let aggregate_pubkey = eth_aggregate_pubkeys(&pubkeys.iter().collect::<Vec<_>>())?;

        Ok(SyncCommittee {
            pubkeys: FixedVector::from(pubkeys),
            aggregate_pubkey,
        })
    }

    pub fn process_sync_committee_updates(&mut self) -> anyhow::Result<()> {
        let next_epoch = self.get_current_epoch() + 1;
        if next_epoch % EPOCHS_PER_SYNC_COMMITTEE_PERIOD == 0 {
            self.current_sync_committee = self.next_sync_committee.clone();
            self.next_sync_committee = Arc::new(self.get_next_sync_committee()?);
        }

        Ok(())
    }

    pub fn process_participation_flag_updates(&mut self) -> anyhow::Result<()> {
        self.previous_epoch_participation = self.current_epoch_participation.clone();
        self.current_epoch_participation = vec![0; self.validators.len()].into();

        Ok(())
    }

    pub fn process_epoch(&mut self) -> anyhow::Result<()> {
        self.process_justification_and_finalization()?;
        self.process_inactivity_updates()?;
        self.process_rewards_and_penalties()?;
        self.process_registry_updates()?;
        self.process_slashings()?;
        self.process_eth1_data_reset()?;
        self.process_pending_deposits()?;
        self.process_pending_consolidations()?;
        self.process_effective_balance_updates()?;
        self.process_slashings_reset()?;
        self.process_randao_mixes_reset()?;
        self.process_historical_summaries_update()?;
        self.process_participation_flag_updates()?;
        self.process_sync_committee_updates()?;

        Ok(())
    }

    pub fn process_slots(&mut self, slot: u64) -> anyhow::Result<()> {
        ensure!(self.slot < slot);

        while self.slot < slot {
            self.process_slot()?;
            // Process epoch on the start slot of the next epoch
            if (self.slot + 1) % SLOTS_PER_EPOCH == 0 {
                self.process_epoch()?;
            }
            self.slot += 1
        }

        Ok(())
    }

    pub fn process_slot(&mut self) -> anyhow::Result<()> {
        // Cache state root
        let previous_state_root = self.tree_hash_root();
        self.state_roots[(self.slot % SLOTS_PER_HISTORICAL_ROOT) as usize] = previous_state_root;
        // Cache latest block header state root
        if self.latest_block_header.state_root == B256::default() {
            self.latest_block_header.state_root = previous_state_root;
        }
        // Cache block root
        let previous_block_root = self.latest_block_header.tree_hash_root();
        self.block_roots[(self.slot % SLOTS_PER_HISTORICAL_ROOT) as usize] = previous_block_root;

        Ok(())
    }

    pub async fn process_execution_payload(
        &mut self,
        body: &BeaconBlockBody,
        execution_engine: &Option<impl ExecutionApi>,
    ) -> anyhow::Result<()> {
        let payload = &body.execution_payload;

        // Verify consistency of the parent hash with respect to the previous execution payload
        // header
        ensure!(payload.parent_hash == self.latest_execution_payload_header.block_hash);
        // Verify prev_randao
        ensure!(payload.prev_randao == self.get_randao_mix(self.get_current_epoch()));
        // Verify timestamp
        ensure!(payload.timestamp == self.compute_timestamp_at_slot(self.slot));
        // Verify commitments are under limit
        ensure!(body.blob_kzg_commitments.len() <= MAX_BLOBS_PER_BLOCK_ELECTRA as usize);

        // Verify the execution payload is valid
        let mut versioned_hashes = vec![];
        for commitment in body.blob_kzg_commitments.iter() {
            versioned_hashes.push(commitment.calculate_versioned_hash());
        }

        if let Some(execution_engine) = execution_engine {
            ensure!(
                execution_engine
                    .verify_and_notify_new_payload(NewPayloadRequest {
                        execution_payload: payload.clone(),
                        versioned_hashes,
                        parent_beacon_block_root: self.latest_block_header.parent_root,
                        execution_requests: body.execution_requests.clone()
                    })
                    .await?
            );
        }

        // Cache execution payload header
        self.latest_execution_payload_header = payload.to_execution_payload_header();

        Ok(())
    }

    pub async fn process_block(
        &mut self,
        block: &BeaconBlock,
        execution_engine: &Option<impl ExecutionApi>,
    ) -> anyhow::Result<()> {
        self.process_block_header(block)?;
        self.process_withdrawals(&block.body.execution_payload)?;
        self.process_execution_payload(&block.body, execution_engine)
            .await?;
        self.process_randao(&block.body)?;
        self.process_eth1_data(&block.body)?;
        self.process_operations(&block.body)?;
        self.process_sync_aggregate(&block.body.sync_aggregate)?;

        Ok(())
    }

    pub async fn state_transition(
        &mut self,
        signed_block: &SignedBeaconBlock,
        validate_result: bool,
        execution_engine: &Option<impl ExecutionApi>,
    ) -> anyhow::Result<()> {
        let block = &signed_block.message;

        // Process slots (including those with no blocks) since block
        self.process_slots(block.slot)?;

        // Verify signature
        if validate_result {
            ensure!(self.verify_block_signature(signed_block)?)
        }

        // Process block
        self.process_block(block, execution_engine).await?;

        // Verify state root
        if validate_result {
            ensure!(block.state_root == self.tree_hash_root())
        }

        Ok(())
    }

    /// Return the churn limit for the current epoch.
    pub fn get_balance_churn_limit(&self) -> u64 {
        let churn = max(
            MIN_PER_EPOCH_CHURN_LIMIT_ELECTRA,
            self.get_total_active_balance() / CHURN_LIMIT_QUOTIENT,
        );
        churn - churn % EFFECTIVE_BALANCE_INCREMENT
    }

    /// Return the churn limit for the current epoch dedicated to activations and exits.
    pub fn get_activation_exit_churn_limit(&self) -> u64 {
        min(
            MAX_PER_EPOCH_ACTIVATION_EXIT_CHURN_LIMIT,
            self.get_balance_churn_limit(),
        )
    }

    pub fn get_consolidation_churn_limit(&self) -> u64 {
        self.get_balance_churn_limit() - self.get_activation_exit_churn_limit()
    }

    pub fn get_pending_balance_to_withdraw(&self, validator_index: u64) -> u64 {
        self.pending_partial_withdrawals
            .iter()
            .filter(|withdrawal| withdrawal.validator_index == validator_index)
            .map(|withdrawal| withdrawal.amount)
            .sum()
    }

    pub fn switch_to_compounding_validator(&mut self, index: u64) -> anyhow::Result<()> {
        let Some(validator) = self.validators.get_mut(index as usize) else {
            return Err(anyhow!("Validator index out of bounds"));
        };

        validator.withdrawal_credentials = B256::from_slice(
            &[
                COMPOUNDING_WITHDRAWAL_PREFIX,
                &validator.withdrawal_credentials[1..],
            ]
            .concat(),
        );
        self.queue_excess_active_balance(index)?;

        Ok(())
    }

    pub fn queue_excess_active_balance(&mut self, index: u64) -> anyhow::Result<()> {
        let Some(balance) = self.balances.get(index as usize) else {
            bail!("Balance index out of bounds");
        };

        if balance > &MIN_ACTIVATION_BALANCE {
            let excess_balance = balance - MIN_ACTIVATION_BALANCE;
            *self
                .balances
                .get_mut(index as usize)
                .ok_or(anyhow!("Balance index out of bounds"))? = MIN_ACTIVATION_BALANCE;

            let Some(validator) = self.validators.get(index as usize) else {
                return Err(anyhow!("Validator index out of bounds"));
            };

            // Use bls.G2_POINT_AT_INFINITY as a signature field placeholder
            // and GENESIS_SLOT to distinguish from a pending deposit request
            self.pending_deposits
                .push(PendingDeposit {
                    pubkey: validator.pubkey.clone(),
                    withdrawal_credentials: validator.withdrawal_credentials,
                    amount: excess_balance,
                    signature: BLSSignature::infinity(),
                    slot: GENESIS_SLOT,
                })
                .map_err(|err| {
                    anyhow!("Failed to push excess active balance to pending deposits: {err:?}")
                })?;
        }

        Ok(())
    }

    pub fn compute_exit_epoch_and_update_churn(&mut self, exit_balance: u64) -> u64 {
        let mut earliest_exit_epoch = max(
            self.earliest_exit_epoch,
            compute_activation_exit_epoch(self.get_current_epoch()),
        );
        let per_epoch_churn = self.get_activation_exit_churn_limit();

        // New epoch for exits.
        let mut exit_balance_to_consume = if self.earliest_exit_epoch < earliest_exit_epoch {
            per_epoch_churn
        } else {
            self.exit_balance_to_consume
        };

        // Exit doesn't fit in the current earliest epoch.
        if exit_balance > exit_balance_to_consume {
            let balance_to_process = exit_balance - exit_balance_to_consume;
            let additional_epochs = (balance_to_process - 1) / per_epoch_churn + 1;
            earliest_exit_epoch += additional_epochs;
            exit_balance_to_consume += additional_epochs * per_epoch_churn;
        }

        // Consume the balance and update the state variables.
        self.exit_balance_to_consume = exit_balance_to_consume - exit_balance;
        self.earliest_exit_epoch = earliest_exit_epoch;

        self.earliest_exit_epoch
    }

    pub fn compute_consolidation_epoch_and_update_churn(
        &mut self,
        consolidation_balance: u64,
    ) -> u64 {
        let mut earliest_consolidation_epoch = max(
            self.earliest_consolidation_epoch,
            compute_activation_exit_epoch(self.get_current_epoch()),
        );
        let per_epoch_churn = self.get_consolidation_churn_limit();

        // New epoch for consolidations.
        let mut consolidation_balance_to_consume =
            if self.earliest_consolidation_epoch < earliest_consolidation_epoch {
                per_epoch_churn
            } else {
                self.consolidation_balance_to_consume
            };

        // Exit doesn't fit in the current earliest epoch.
        if consolidation_balance > consolidation_balance_to_consume {
            let balance_to_process = consolidation_balance - consolidation_balance_to_consume;
            let additional_epochs = (balance_to_process - 1) / per_epoch_churn + 1;
            earliest_consolidation_epoch += additional_epochs;
            consolidation_balance_to_consume += additional_epochs * per_epoch_churn;
        }

        // Consume the balance and update the state variables.
        self.consolidation_balance_to_consume =
            consolidation_balance_to_consume - consolidation_balance;
        self.earliest_consolidation_epoch = earliest_consolidation_epoch;

        self.earliest_consolidation_epoch
    }

    pub fn merkle_leaves(&self) -> Vec<B256> {
        vec![
            self.genesis_time.to_le_bytes().tree_hash_root(),
            self.genesis_validators_root.tree_hash_root(),
            self.slot.to_le_bytes().tree_hash_root(),
            self.fork.tree_hash_root(),
            self.latest_block_header.tree_hash_root(),
            self.block_roots.tree_hash_root(),
            self.state_roots.tree_hash_root(),
            self.historical_roots.tree_hash_root(),
            self.eth1_data.tree_hash_root(),
            self.eth1_data_votes.tree_hash_root(),
            self.eth1_deposit_index.to_le_bytes().tree_hash_root(),
            self.validators.tree_hash_root(),
            self.balances.tree_hash_root(),
            self.randao_mixes.tree_hash_root(),
            self.slashings.tree_hash_root(),
            self.previous_epoch_participation.tree_hash_root(),
            self.current_epoch_participation.tree_hash_root(),
            self.justification_bits.tree_hash_root(),
            self.previous_justified_checkpoint.tree_hash_root(),
            self.current_justified_checkpoint.tree_hash_root(),
            self.finalized_checkpoint.tree_hash_root(),
            self.inactivity_scores.tree_hash_root(),
            self.current_sync_committee.tree_hash_root(),
            self.next_sync_committee.tree_hash_root(),
            self.latest_execution_payload_header.tree_hash_root(),
            self.next_withdrawal_index.to_le_bytes().tree_hash_root(),
            self.next_withdrawal_validator_index
                .to_le_bytes()
                .tree_hash_root(),
            self.historical_summaries.tree_hash_root(),
            self.deposit_requests_start_index
                .to_le_bytes()
                .tree_hash_root(),
            self.deposit_balance_to_consume
                .to_le_bytes()
                .tree_hash_root(),
            self.exit_balance_to_consume.to_le_bytes().tree_hash_root(),
            self.earliest_exit_epoch.to_le_bytes().tree_hash_root(),
            self.consolidation_balance_to_consume
                .to_le_bytes()
                .tree_hash_root(),
            self.earliest_consolidation_epoch
                .to_le_bytes()
                .tree_hash_root(),
            self.pending_deposits.tree_hash_root(),
            self.pending_partial_withdrawals.tree_hash_root(),
            self.pending_consolidations.tree_hash_root(),
        ]
    }

    pub fn data_inclusion_proof(&self, index: u64) -> anyhow::Result<Vec<B256>> {
        let tree = merkle_tree(&self.merkle_leaves(), BEACON_STATE_MERKLE_DEPTH)?;
        generate_proof(&tree, index, BEACON_STATE_MERKLE_DEPTH)
    }

    pub fn current_sync_committee_inclusion_proof(&self) -> anyhow::Result<Vec<B256>> {
        self.data_inclusion_proof(CURRENT_SYNC_COMMITTEE_INDEX)
    }

    pub fn next_sync_committee_inclusion_proof(&self) -> anyhow::Result<Vec<B256>> {
        self.data_inclusion_proof(NEXT_SYNC_COMMITTEE_INDEX)
    }

    pub fn finalized_root_inclusion_proof(&self) -> anyhow::Result<Vec<B256>> {
        let root_to_finalized_checkpoint_proof = vec![
            self.finalized_checkpoint
                .epoch
                .to_le_bytes()
                .tree_hash_root(),
        ];
        let finalized_checkpoint_to_beacon_state_proof =
            self.data_inclusion_proof(FINALIZED_CHECKPOINT_INDEX)?;
        Ok([
            root_to_finalized_checkpoint_proof,
            finalized_checkpoint_to_beacon_state_proof,
        ]
        .concat())
    }

    pub fn state_root(&self) -> B256 {
        self.tree_hash_root()
    }
}

pub fn get_validator_from_deposit(
    pubkey: PubKey,
    withdrawal_credentials: B256,
    amount: u64,
) -> Validator {
    let mut validator = Validator {
        pubkey,
        withdrawal_credentials,
        effective_balance: 0,
        slashed: false,
        activation_eligibility_epoch: FAR_FUTURE_EPOCH,
        activation_epoch: FAR_FUTURE_EPOCH,
        exit_epoch: FAR_FUTURE_EPOCH,
        withdrawable_epoch: FAR_FUTURE_EPOCH,
    };

    let max_effective_balance = validator.get_max_effective_balance();
    validator.effective_balance = min(
        amount - amount % EFFECTIVE_BALANCE_INCREMENT,
        max_effective_balance,
    );
    validator
}

/// Wrapper to ``bls.FastAggregateVerify`` accepting the ``G2_POINT_AT_INFINITY`` signature when
/// ``pubkeys`` is empty.
pub fn eth_fast_aggregate_verify(
    pubkeys: &[&PubKey],
    message: B256,
    signature: &BLSSignature,
) -> anyhow::Result<bool> {
    if pubkeys.is_empty() && *signature == BLSSignature::infinity() {
        return Ok(true);
    }

    signature
        .fast_aggregate_verify(pubkeys, message.as_ref())
        .map_err(|e| anyhow!("Failed to verify fast aggregate: {:?}", e))
}

/// Return the aggregate public key for the public keys in ``pubkeys``.
/// NOTE: the ``+`` operation should be interpreted as elliptic curve point addition, which takes as
/// input elliptic curve points that must be decoded from the input ``BLSPubkey``s.
/// This implementation is for demonstrative purposes only and ignores encoding/decoding concerns.
/// Refer to the BLS signature draft standard for more information.
pub fn eth_aggregate_pubkeys(pubkeys: &[&PubKey]) -> anyhow::Result<PubKey> {
    ensure!(!pubkeys.is_empty(), "Public keys list cannot be empty");

    let aggregate_pubkey = AggregatePubKey::aggregate(pubkeys)?;
    Ok(aggregate_pubkey.to_pubkey())
}

/// Return the largest integer ``x`` such that ``x**2 <= n``.
pub fn integer_squareroot(n: u64) -> u64 {
    if n == UINT64_MAX {
        return UINT64_MAX_SQRT;
    }

    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

pub fn is_valid_deposit_signature(
    pubkey: &PubKey,
    withdrawal_credentials: B256,
    amount: u64,
    signature: &BLSSignature,
) -> anyhow::Result<bool> {
    let deposit_message = DepositMessage {
        pubkey: pubkey.clone(),
        withdrawal_credentials,
        amount,
    };
    // Fork-agnostic domain since deposits are valid across forks
    let domain = compute_domain(DOMAIN_DEPOSIT, None, None);
    let signing_root = compute_signing_root(deposit_message, domain);

    signature
        .verify(pubkey, signing_root.as_ref())
        .map_err(|err| anyhow!("Invalid deposit signature: {err:?}"))
}
