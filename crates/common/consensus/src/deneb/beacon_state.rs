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
    deposit::Deposit,
    deposit_message::DepositMessage,
    eth_1_data::Eth1Data,
    fork::Fork,
    fork_choice::helpers::constants::{
        BASE_REWARD_FACTOR, BLS_WITHDRAWAL_PREFIX, CAPELLA_FORK_VERSION, CHURN_LIMIT_QUOTIENT,
        DEPOSIT_CONTRACT_TREE_DEPTH, DOMAIN_BEACON_ATTESTER, DOMAIN_BEACON_PROPOSER,
        DOMAIN_BLS_TO_EXECUTION_CHANGE, DOMAIN_DEPOSIT, DOMAIN_RANDAO, DOMAIN_SYNC_COMMITTEE,
        DOMAIN_VOLUNTARY_EXIT, EFFECTIVE_BALANCE_INCREMENT, EJECTION_BALANCE,
        EPOCHS_PER_ETH1_VOTING_PERIOD, EPOCHS_PER_HISTORICAL_VECTOR, EPOCHS_PER_SLASHINGS_VECTOR,
        ETH1_ADDRESS_WITHDRAWAL_PREFIX, FAR_FUTURE_EPOCH, G2_POINT_AT_INFINITY, GENESIS_EPOCH,
        GENESIS_SLOT, HYSTERESIS_DOWNWARD_MULTIPLIER, HYSTERESIS_QUOTIENT,
        HYSTERESIS_UPWARD_MULTIPLIER, INACTIVITY_PENALTY_QUOTIENT_ALTAIR, INACTIVITY_SCORE_BIAS,
        INACTIVITY_SCORE_RECOVERY_RATE, JUSTIFICATION_BITS_LENGTH, MAX_COMMITTEES_PER_SLOT,
        MAX_DEPOSITS, MAX_EFFECTIVE_BALANCE, MAX_PER_EPOCH_ACTIVATION_CHURN_LIMIT, MAX_RANDOM_BYTE,
        MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP, MAX_WITHDRAWALS_PER_PAYLOAD,
        MIN_ATTESTATION_INCLUSION_DELAY, MIN_EPOCHS_TO_INACTIVITY_PENALTY,
        MIN_GENESIS_ACTIVE_VALIDATOR_COUNT, MIN_GENESIS_TIME, MIN_PER_EPOCH_CHURN_LIMIT,
        MIN_SEED_LOOKAHEAD, MIN_SLASHING_PENALTY_QUOTIENT, MIN_VALIDATOR_WITHDRAWABILITY_DELAY,
        PARTICIPATION_FLAG_WEIGHTS, PROPORTIONAL_SLASHING_MULTIPLIER_BELLATRIX,
        PROPOSER_REWARD_QUOTIENT, PROPOSER_WEIGHT, SECONDS_PER_SLOT, SHARD_COMMITTEE_PERIOD,
        SLOTS_PER_EPOCH, SLOTS_PER_HISTORICAL_ROOT, SYNC_COMMITTEE_SIZE, SYNC_REWARD_WEIGHT,
        TARGET_COMMITTEE_SIZE, TIMELY_HEAD_FLAG_INDEX, TIMELY_SOURCE_FLAG_INDEX,
        TIMELY_TARGET_FLAG_INDEX, WEIGHT_DENOMINATOR, WHISTLEBLOWER_REWARD_QUOTIENT,
    },
    helpers::xor,
    historical_summary::HistoricalSummary,
    indexed_attestation::IndexedAttestation,
    misc::{
        compute_activation_exit_epoch, compute_committee, compute_domain, compute_epoch_at_slot,
        compute_shuffled_index, compute_signing_root, compute_start_slot_at_epoch,
        is_sorted_and_unique,
    },
    predicates::is_slashable_attestation_data,
    proposer_slashing::ProposerSlashing,
    pubkey::PubKey,
    signature::BlsSignature,
    sync_aggregate::SyncAggregate,
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
        .map_err(|err| anyhow!("Domain computation failed: {:?}", err))
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

        // Collect public keys of attesting validators
        let pubkeys: Vec<_> = indices
            .iter()
            .filter_map(|&i| self.validators.get(i).map(|v| v.pubkey.clone()))
            .collect();

        // Compute domain and signing root
        let domain = self.get_domain(
            DOMAIN_BEACON_ATTESTER,
            Some(indexed_attestation.data.target.epoch),
        )?;

        let sig = blst::min_pk::Signature::from_bytes(&indexed_attestation.signature.signature)
            .map_err(|err| anyhow!("Signarure conversion failed:{:?}", err))?;
        let signing_root = compute_signing_root(&indexed_attestation.data, domain);

        let publickeys = pubkeys
            .iter()
            .map(|key| {
                blst::min_pk::PublicKey::from_bytes(&key.inner)
                    .map_err(|e| anyhow!(format!("Public key conversion failed:{:?}", e)))
            })
            .collect::<Result<Vec<blst::min_pk::PublicKey>, _>>()?;

        let verification_result = sig.fast_aggregate_verify(
            true,
            signing_root.as_ref(),
            DST,
            publickeys.iter().collect::<Vec<_>>().as_slice(),
        );

        Ok(matches!(
            verification_result,
            blst::BLST_ERROR::BLST_SUCCESS
        ))
    }

    /// Return the set of attesting indices corresponding to ``data`` and ``bits``.
    pub fn get_attesting_indices(&self, attestation: &Attestation) -> anyhow::Result<Vec<u64>> {
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
        attestation: &Attestation,
    ) -> anyhow::Result<IndexedAttestation> {
        let mut attesting_indices = self.get_attesting_indices(attestation)?;
        attesting_indices.sort();
        Ok(IndexedAttestation {
            attesting_indices: attesting_indices.into(),
            data: attestation.data.clone(),
            signature: attestation.signature.clone(),
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
            if v.is_active_validator(previous_epoch)
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
            )?;

            let signing_root = compute_signing_root(&signed_header.message, domain);

            let sig = blst::min_pk::Signature::from_bytes(&signed_header.signature.signature)
                .map_err(|err| anyhow!("Unable to retrieve BLS Signature from byets, {:?}", err))?;

            let public_key = PublicKey::from_bytes(&proposer.pubkey.inner)
                .map_err(|err| anyhow!("Unable to convert PublicKey, {:?}", err))?;

            let verification_result =
                sig.fast_aggregate_verify(true, signing_root.as_ref(), DST, &[&public_key]);

            ensure!(
                verification_result == blst::BLST_ERROR::BLST_SUCCESS,
                "BLS Signature verification failed!"
            );
        }

        // Slash the validator
        self.slash_validator(proposer_index, None)
    }

    pub fn process_historical_summaries_update(&mut self) -> anyhow::Result<()> {
        // Set historical block root accumulator.
        let next_epoch = self.get_current_epoch() + 1;
        if next_epoch % SLOTS_PER_HISTORICAL_ROOT / SLOTS_PER_EPOCH == 0 {
            let historical_summary = HistoricalSummary {
                block_summary_root: self.block_roots.tree_hash_root(),
                state_summary_root: self.state_roots.tree_hash_root(),
            };
            self.historical_summaries
                .push(historical_summary)
                .map_err(|err| anyhow!("Failed to push historical summory {err:?}"))?;
        }
        Ok(())
    }

    pub fn process_attester_slashing(
        &mut self,
        attester_slashing: AttesterSlashing,
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

    pub fn process_sync_aggregate(&mut self, sync_aggregate: SyncAggregate) -> anyhow::Result<()> {
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
        )?;
        let signing_root =
            compute_signing_root(self.get_block_root_at_slot(previous_slot)?, domain);

        let is_valid = eth_fast_aggregate_verify(
            &participant_pubkeys,
            signing_root,
            &sync_aggregate.sync_committee_signature,
        )?;

        ensure!(is_valid, "Sync aggregate signature verification failed.");

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
                self.increase_balance(*participant_index as u64, participant_reward);
                self.increase_balance(self.get_beacon_proposer_index()?, proposer_reward);
            } else {
                self.decrease_balance(*participant_index as u64, participant_reward);
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

        // Process justifications
        self.previous_justified_checkpoint = self.current_justified_checkpoint;
        for i in 1..JUSTIFICATION_BITS_LENGTH as usize {
            let bit = self
                .justification_bits
                .get(i - 1)
                .map_err(|err| anyhow!("Failed to get justification bit {err:?}"))?;
            self.justification_bits
                .set(i, bit)
                .map_err(|err| anyhow!("Failed to set justification bits {err:?}"))?;
        }
        self.justification_bits
            .set(0, false)
            .map_err(|err| anyhow!("Failed to set justification bit 0 {err:?}"))?;

        if previous_epoch_target_balance * 3 >= total_active_balance * 2 {
            self.current_justified_checkpoint = Checkpoint {
                epoch: previous_epoch,
                root: self.get_block_root(previous_epoch)?,
            };
            self.justification_bits
                .set(1, true)
                .map_err(|err| anyhow!("Failed to set justification {err:?}"))?;
        }

        if current_epoch_target_balance * 3 >= total_active_balance * 2 {
            self.current_justified_checkpoint = Checkpoint {
                epoch: current_epoch,
                root: self.get_block_root(current_epoch)?,
            };
            self.justification_bits
                .set(0, true)
                .map_err(|err| anyhow!("Failed to set justification bit {err:?}"))?;
        }

        let bits = &self.justification_bits;
        let bits: Vec<bool> = bits.iter().collect();
        if bits[1..4].iter().all(|&b| b)
            && old_previous_justified_checkpoint.epoch + 3 == current_epoch
        {
            self.finalized_checkpoint = old_previous_justified_checkpoint;
        }

        if bits[1..3].iter().all(|&b| b)
            && old_previous_justified_checkpoint.epoch + 2 == current_epoch
        {
            self.finalized_checkpoint = old_previous_justified_checkpoint;
        }

        if bits[0..3].iter().all(|&b| b)
            && old_current_justified_checkpoint.epoch + 2 == current_epoch
        {
            self.finalized_checkpoint = old_current_justified_checkpoint;
        }

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
                validator.effective_balance =
                    (balance - balance % EFFECTIVE_BALANCE_INCREMENT).min(MAX_EFFECTIVE_BALANCE);
            }
        }
        Ok(())
    }

    pub fn process_randao(&mut self, body: BeaconBlockBody) -> anyhow::Result<()> {
        let epoch = self.get_current_epoch();

        // Verify RANDAO reveal
        if let Some(proposer) = self
            .validators
            .get(self.get_beacon_proposer_index()? as usize)
        {
            let signing_root =
                compute_signing_root(epoch, self.get_domain(DOMAIN_RANDAO, Some(epoch))?);

            ensure!(eth_fast_aggregate_verify(
                &[&proposer.pubkey],
                signing_root,
                &body.randao_reveal
            )?);

            // Mix in RANDAO reveal
            let mix = xor(
                self.get_randao_mix(epoch).as_slice(),
                hash(&body.randao_reveal.signature).as_slice(),
            );
            self.randao_mixes[(epoch % EPOCHS_PER_HISTORICAL_VECTOR) as usize] = mix;
        }

        Ok(())
    }

    pub fn process_eth1_data(&mut self, body: BeaconBlockBody) -> anyhow::Result<()> {
        self.eth1_data_votes
            .push(body.eth1_data.clone())
            .map_err(|err| anyhow!("Can't push eth1_data {err:?}"))?;

        let count = self
            .eth1_data_votes
            .iter()
            .filter(|data| **data == body.eth1_data)
            .count() as u64;

        if count * 2 > (EPOCHS_PER_ETH1_VOTING_PERIOD * SLOTS_PER_EPOCH) {
            self.eth1_data = body.eth1_data;
        }

        Ok(())
    }

    pub fn process_attestation(&mut self, attestation: &Attestation) -> anyhow::Result<()> {
        ensure!(
            attestation.data.target.epoch == self.get_previous_epoch()
                || attestation.data.target.epoch == self.get_current_epoch(),
            "Target epoch must be the previous or current epoch"
        );

        ensure!(
            attestation.data.target.epoch == compute_epoch_at_slot(attestation.data.slot),
            "Target epoch must match the computed epoch at slot"
        );

        ensure!(
            attestation.data.slot + MIN_ATTESTATION_INCLUSION_DELAY <= self.slot,
            "Attestation must be included after the minimum delay"
        );

        ensure!(
            attestation.data.index
                < self.get_committee_count_per_slot(attestation.data.target.epoch),
            "Committee index must be within bounds"
        );

        let committee = self.get_beacon_committee(attestation.data.slot, attestation.data.index)?;
        ensure!(
            attestation.aggregation_bits.len() == committee.len(),
            "Aggregation bits length must match committee size"
        );

        let participation_flag_indices = self.get_attestation_participation_flag_indices(
            &attestation.data,
            self.slot - attestation.data.slot,
        )?;

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
        let epoch_participation = if attestation.data.target.epoch == self.get_current_epoch() {
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

        let proposer_reward_denominator =
            (WEIGHT_DENOMINATOR - PROPOSER_WEIGHT) * WEIGHT_DENOMINATOR / PROPOSER_WEIGHT;
        let proposer_reward = proposer_reward_numerator / proposer_reward_denominator;
        self.increase_balance(self.get_beacon_proposer_index()?, proposer_reward);
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

        for index in 0..self.validators.len() {
            let validator = &self.validators[index];
            if validator.slashed
                && epoch + EPOCHS_PER_SLASHINGS_VECTOR / 2 == validator.withdrawable_epoch
            {
                let increment = EFFECTIVE_BALANCE_INCREMENT; // Factored out from penalty numerator to avoid uint64 overflow
                let penalty_numerator =
                    validator.effective_balance / increment * adjusted_total_slashing_balance;
                let penalty = penalty_numerator / total_balance * increment;

                self.decrease_balance(index as u64, penalty);
            }
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

    pub fn process_operations(&mut self, body: BeaconBlockBody) -> anyhow::Result<()> {
        // Verify that outstanding deposits are processed up to the maximum number of deposits
        ensure!(
            body.deposits.len()
                == min(
                    MAX_DEPOSITS as usize,
                    (self.eth1_data.deposit_count - self.eth1_deposit_index) as usize
                )
        );

        for operation in body.proposer_slashings {
            self.process_proposer_slashing(&operation)?;
        }
        for operation in body.attester_slashings {
            self.process_attester_slashing(operation)?;
        }
        for operation in body.attestations {
            self.process_attestation(&operation)?;
        }
        for operation in body.deposits {
            self.process_deposit(operation)?;
        }
        for operation in body.voluntary_exits {
            self.process_voluntary_exit(operation)?;
        }
        for operation in body.bls_to_execution_changes {
            self.process_bls_to_execution_change(operation)?;
        }

        Ok(())
    }

    pub fn verify_block_signature(&self, signed_block: SignedBeaconBlock) -> anyhow::Result<bool> {
        let proposer = &self.validators[signed_block.message.proposer_index as usize];
        let signing_root = compute_signing_root(
            signed_block.message,
            self.get_domain(DOMAIN_BEACON_PROPOSER, None)?,
        );
        let sig = blst::min_pk::Signature::from_bytes(&signed_block.signature.signature).map_err(
            |err| {
                anyhow!(
                    "Failed to convert signature to BLS Signature type, {:?}",
                    err
                )
            },
        )?;
        let public_key = PublicKey::from_bytes(&proposer.pubkey.inner)
            .map_err(|err| anyhow!("Failed to convert pubkey to BLS PublicKey type, {:?}", err))?;
        let verification_result =
            sig.fast_aggregate_verify(true, signing_root.as_ref(), DST, &[&public_key]);
        Ok(matches!(
            verification_result,
            blst::BLST_ERROR::BLST_SUCCESS
        ))
    }

    /// Check if ``validator`` is eligible for activation.
    pub fn is_eligible_for_activation(&self, validator: &Validator) -> bool {
        // Placement in queue is finalized
        validator.activation_eligibility_epoch <= self.finalized_checkpoint.epoch
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
        let mut initiate_validator = vec![];

        // Process activation eligibility and ejections
        for (index, validator) in self.validators.iter_mut().enumerate() {
            if validator.is_eligible_for_activation_queue() {
                validator.activation_eligibility_epoch = current_epoch + 1;
            }

            if validator.is_active_validator(current_epoch)
                && validator.effective_balance <= EJECTION_BALANCE
            {
                initiate_validator.push(index as u64);
            }
        }

        for index in initiate_validator {
            self.initiate_validator_exit(index);
        }

        // Queue validators eligible for activation and not yet dequeued for activation
        let mut activation_queue: Vec<usize> = (0..self.validators.len())
            // Order by the sequence of activation_eligibility_epoch setting and then index
            .filter(|&index| self.is_eligible_for_activation(&self.validators[index]))
            .collect();

        activation_queue
            .sort_by_key(|&index| (self.validators[index].activation_eligibility_epoch, index));

        // Dequeued validators for activation up to activation churn limit
        // [Modified in Deneb:EIP7514]
        for index in activation_queue[..self.get_validator_activation_churn_limit() as usize].iter()
        {
            self.validators[*index].activation_epoch = compute_activation_exit_epoch(current_epoch);
        }

        Ok(())
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

/// Wrapper to ``bls.FastAggregateVerify`` accepting the ``G2_POINT_AT_INFINITY`` signature when
/// ``pubkeys`` is empty.
pub fn eth_fast_aggregate_verify(
    pubkeys: &[&PubKey],
    message: B256,
    signature: &BlsSignature,
) -> anyhow::Result<bool> {
    if pubkeys.is_empty() && *signature == G2_POINT_AT_INFINITY {
        return Ok(true);
    }

    let public_keys: Vec<blst::min_pk::PublicKey> = pubkeys
        .iter()
        .map(|key| {
            blst::min_pk::PublicKey::from_bytes(&key.inner)
                .map_err(|err| anyhow!("Could not parse public key: {err:?}"))
        })
        .collect::<Result<_, _>>()?;

    let sig = blst::min_pk::Signature::from_bytes(&signature.signature)
        .map_err(|err| anyhow!("Could not parse signature: {err:?}"))?;

    let message = message.as_ref();

    Ok(
        sig.fast_aggregate_verify(true, message, DST, &public_keys.iter().collect::<Vec<_>>())
            == blst::BLST_ERROR::BLST_SUCCESS,
    )
}
