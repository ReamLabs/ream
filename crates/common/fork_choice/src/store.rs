use std::collections::hash_map::Entry;

use alloy_primitives::{B256, map::HashMap};
use anyhow::{anyhow, ensure};
use ream_consensus::{
    attestation::Attestation,
    blob_sidecar::BlobIdentifier,
    checkpoint::Checkpoint,
    constants::{
        GENESIS_EPOCH, GENESIS_SLOT, INTERVALS_PER_SLOT, SECONDS_PER_SLOT, SLOTS_PER_EPOCH,
    },
    deneb::{beacon_block::BeaconBlock, beacon_state::BeaconState},
    execution_engine::{engine_trait::ExecutionApi, rpc_types::get_blobs::BlobAndProofV1},
    fork_choice::latest_message::LatestMessage,
    helpers::{calculate_committee_fraction, get_total_active_balance},
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch, is_shuffling_stable},
    polynomial_commitments::kzg_commitment::KZGCommitment,
};
use ream_polynomial_commitments::handlers::verify_blob_kzg_proof_batch;
use serde::Deserialize;
use tree_hash::TreeHash;

use crate::constants::{
    PROPOSER_SCORE_BOOST, REORG_HEAD_WEIGHT_THRESHOLD, REORG_MAX_EPOCHS_SINCE_FINALIZATION,
    REORG_PARENT_WEIGHT_THRESHOLD,
};

#[derive(Debug, PartialEq, Deserialize)]
pub struct Store {
    pub time: u64,
    pub genesis_time: u64,
    pub justified_checkpoint: Checkpoint,
    pub finalized_checkpoint: Checkpoint,
    pub unrealized_justified_checkpoint: Checkpoint,
    pub unrealized_finalized_checkpoint: Checkpoint,
    pub proposer_boost_root: B256,
    pub equivocating_indices: Vec<u64>,
    pub blocks: HashMap<B256, BeaconBlock>,
    pub block_states: HashMap<B256, BeaconState>,
    pub block_timeliness: HashMap<B256, bool>,
    pub checkpoint_states: HashMap<Checkpoint, BeaconState>,
    pub latest_messages: HashMap<u64, LatestMessage>,
    pub unrealized_justifications: HashMap<B256, Checkpoint>,
    pub blobs_and_proofs: HashMap<BlobIdentifier, BlobAndProofV1>,
}

impl Store {
    pub fn is_previous_epoch_justified(&self) -> bool {
        let current_epoch = self.get_current_store_epoch();
        self.justified_checkpoint.epoch + 1 == current_epoch
    }

    pub fn get_current_store_epoch(&self) -> u64 {
        compute_epoch_at_slot(self.get_current_slot())
    }

    pub fn get_current_slot(&self) -> u64 {
        GENESIS_SLOT + self.get_slots_since_genesis()
    }

    pub fn get_slots_since_genesis(&self) -> u64 {
        (self.time - self.genesis_time) / SECONDS_PER_SLOT
    }

    pub fn get_ancestor(&self, root: B256, slot: u64) -> anyhow::Result<B256> {
        let block = self
            .blocks
            .get(&root)
            .ok_or(anyhow!("Failed to find root in blocks"))?;
        if block.slot > slot {
            self.get_ancestor(block.parent_root, slot)
        } else {
            Ok(root)
        }
    }

    pub fn get_checkpoint_block(&self, root: B256, epoch: u64) -> anyhow::Result<B256> {
        let epoch_first_slot = compute_start_slot_at_epoch(epoch);
        self.get_ancestor(root, epoch_first_slot)
    }

    pub fn filter_block_tree(
        &self,
        block_root: B256,
        blocks: &mut HashMap<B256, BeaconBlock>,
    ) -> anyhow::Result<bool> {
        let block = &self.blocks[&block_root];

        let children: Vec<B256> = self
            .blocks
            .keys()
            .filter(|&root| self.blocks[root].parent_root == block_root)
            .cloned()
            .collect();

        if !children.is_empty() {
            let filter_results = children
                .iter()
                .map(|child| self.filter_block_tree(*child, blocks))
                .collect::<anyhow::Result<Vec<_>>>()?;

            if filter_results.iter().any(|&result| result) {
                blocks.insert(block_root, block.clone());
                return Ok(true);
            }
            return Ok(false);
        }

        let current_epoch = self.get_current_store_epoch();
        let voting_source = self.get_voting_source(block_root);

        let correct_justified = self.justified_checkpoint.epoch == GENESIS_EPOCH || {
            voting_source.epoch == self.justified_checkpoint.epoch
                || voting_source.epoch + 2 >= current_epoch
        };

        let finalized_checkpoint_block =
            self.get_checkpoint_block(block_root, self.finalized_checkpoint.epoch)?;

        let correct_finalized = self.finalized_checkpoint.epoch == GENESIS_EPOCH
            || self.finalized_checkpoint.root == finalized_checkpoint_block;

        if correct_justified && correct_finalized {
            blocks.insert(block_root, block.clone());
            return Ok(true);
        }

        Ok(false)
    }

    pub fn update_checkpoints(
        &mut self,
        justified_checkpoint: Checkpoint,
        finalized_checkpoint: Checkpoint,
    ) {
        if justified_checkpoint.epoch > self.justified_checkpoint.epoch {
            self.justified_checkpoint = justified_checkpoint;
        }

        if finalized_checkpoint.epoch > self.finalized_checkpoint.epoch {
            self.finalized_checkpoint = finalized_checkpoint;
        }
    }

    pub fn update_unrealized_checkpoints(
        &mut self,
        unrealized_justified_checkpoint: Checkpoint,
        unrealized_finalized_checkpoint: Checkpoint,
    ) {
        if unrealized_justified_checkpoint.epoch > self.unrealized_justified_checkpoint.epoch {
            self.unrealized_justified_checkpoint = unrealized_justified_checkpoint;
        }

        if unrealized_finalized_checkpoint.epoch > self.unrealized_finalized_checkpoint.epoch {
            self.unrealized_finalized_checkpoint = unrealized_finalized_checkpoint;
        }
    }

    // Helper functions
    pub fn is_head_late(&self, head_root: B256) -> bool {
        !self.block_timeliness.get(&head_root).unwrap_or(&true)
    }

    pub fn is_ffg_competitive(&self, head_root: B256, parent_root: B256) -> bool {
        self.unrealized_justifications.get(&head_root)
            == self.unrealized_justifications.get(&parent_root)
    }

    pub fn is_proposing_on_time(&self) -> bool {
        let time_into_slot = (self.time - self.genesis_time) % SECONDS_PER_SLOT;
        let proposer_reorg_cutoff = SECONDS_PER_SLOT / INTERVALS_PER_SLOT / 2;
        time_into_slot <= proposer_reorg_cutoff
    }

    pub fn is_finalization_ok(&self, slot: u64) -> bool {
        let epochs_since_finalization =
            compute_epoch_at_slot(slot) - self.finalized_checkpoint.epoch;
        epochs_since_finalization <= REORG_MAX_EPOCHS_SINCE_FINALIZATION
    }

    pub fn get_proposer_score(&self) -> anyhow::Result<u64> {
        let justified_checkpoint_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .ok_or(anyhow!("Failed to find checkpoint in checkpoint states"))?;
        let committee_weight =
            get_total_active_balance(justified_checkpoint_state) / SLOTS_PER_EPOCH;
        Ok((committee_weight * PROPOSER_SCORE_BOOST) / 100)
    }

    pub fn get_weight(&self, root: B256) -> anyhow::Result<u64> {
        let state = &self.checkpoint_states[&self.justified_checkpoint];

        let unslashed_and_active_indices: Vec<u64> = state
            .get_active_validator_indices(state.get_current_epoch())
            .into_iter()
            .filter(|&i| !state.validators[i as usize].slashed)
            .collect();

        let mut attestation_score: u64 = 0;
        for index in unslashed_and_active_indices {
            if self.latest_messages.contains_key(&index)
                && !self.equivocating_indices.contains(&index)
                && self.get_ancestor(self.latest_messages[&index].root, self.blocks[&root].slot)?
                    == root
            {
                attestation_score += state.validators[index as usize].effective_balance;
            }
        }

        if self.proposer_boost_root == B256::ZERO {
            return Ok(attestation_score);
        }

        let mut proposer_score: u64 = 0;
        if self.get_ancestor(self.proposer_boost_root, self.blocks[&root].slot)? == root {
            proposer_score = self.get_proposer_score()?;
        }

        Ok(attestation_score + proposer_score)
    }

    pub fn get_voting_source(&self, block_root: B256) -> Checkpoint {
        let block = &self.blocks[&block_root];

        let current_epoch = self.get_current_store_epoch();
        let block_epoch = compute_epoch_at_slot(block.slot);

        if current_epoch > block_epoch {
            self.unrealized_justifications[&block_root]
        } else {
            let head_state = &self.block_states[&block_root];
            head_state.current_justified_checkpoint
        }
    }

    pub fn is_head_weak(&self, head_root: B256) -> anyhow::Result<bool> {
        let justified_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .ok_or(anyhow!("Justified checkpoint must exist in the store"))?;

        let reorg_threshold =
            calculate_committee_fraction(justified_state, REORG_HEAD_WEIGHT_THRESHOLD);
        let head_weight = self.get_weight(head_root)?;

        Ok(head_weight < reorg_threshold)
    }

    pub fn is_parent_strong(&self, parent_root: B256) -> anyhow::Result<bool> {
        let justified_state = self
            .checkpoint_states
            .get(&self.justified_checkpoint)
            .ok_or(anyhow!("Justified checkpoint must exist in the store"))?;

        let parent_threshold =
            calculate_committee_fraction(justified_state, REORG_PARENT_WEIGHT_THRESHOLD);
        let parent_weight = self.get_weight(parent_root)?;

        Ok(parent_weight > parent_threshold)
    }

    pub fn get_proposer_head(&self, head_root: B256, slot: u64) -> anyhow::Result<B256> {
        let head_block = self
            .blocks
            .get(&head_root)
            .ok_or(anyhow!("Head block must exist"))?;
        let parent_root = head_block.parent_root;
        let parent_block = self
            .blocks
            .get(&parent_root)
            .ok_or(anyhow!("Parent block must exist"))?;

        let head_late = self.is_head_late(head_root);

        let shuffling_stable = is_shuffling_stable(slot);

        let ffg_competitive = self.is_ffg_competitive(head_root, parent_root);

        let finalization_ok = self.is_finalization_ok(slot);

        let proposing_on_time = self.is_proposing_on_time();

        let parent_slot_ok = parent_block.slot + 1 == head_block.slot;
        let current_time_ok = head_block.slot + 1 == slot;
        let single_slot_reorg = parent_slot_ok && current_time_ok;

        assert!(self.proposer_boost_root != head_root); // Ensure boost has worn off
        let head_weak = self.is_head_weak(head_root)?;

        let parent_strong = self.is_parent_strong(parent_root)?;

        if head_late
            && shuffling_stable
            && ffg_competitive
            && finalization_ok
            && proposing_on_time
            && single_slot_reorg
            && head_weak
            && parent_strong
        {
            Ok(parent_root)
        } else {
            Ok(head_root)
        }
    }

    pub fn update_latest_messages(
        &mut self,
        attesting_indices: Vec<u64>,
        attestation: Attestation,
    ) -> anyhow::Result<()> {
        let target = attestation.data.target;
        let beacon_block_root = attestation.data.beacon_block_root;
        let mut non_equivocating_attesting_indices = vec![];

        for &index in &attesting_indices {
            if !self.equivocating_indices.contains(&index) {
                non_equivocating_attesting_indices.push(index);
            }
        }

        for index in &non_equivocating_attesting_indices {
            if !self.latest_messages.contains_key(index)
                || target.epoch
                    > self
                        .latest_messages
                        .get(index)
                        .ok_or(anyhow!(
                            "Could not get expected latest message at index: {index}"
                        ))?
                        .epoch
            {
                self.latest_messages.insert(
                    *index,
                    LatestMessage {
                        epoch: target.epoch,
                        root: beacon_block_root,
                    },
                );
            }
        }

        Ok(())
    }

    pub fn on_tick_per_slot(&mut self, time: u64) -> anyhow::Result<()> {
        let previous_slot = self.get_current_slot();

        // Update store time
        self.time = time;

        let current_slot = self.get_current_slot();

        // If this is a new slot, reset store.proposer_boost_root
        if current_slot > previous_slot {
            self.proposer_boost_root = B256::ZERO;
        }

        // If a new epoch, pull-up justification and finalization from previous epoch
        if current_slot > previous_slot && compute_slots_since_epoch_start(current_slot) == 0 {
            self.update_checkpoints(
                self.unrealized_justified_checkpoint,
                self.unrealized_finalized_checkpoint,
            );
        }

        Ok(())
    }

    pub fn validate_target_epoch_against_current_time(
        &mut self,
        attestation: &Attestation,
    ) -> anyhow::Result<()> {
        let target = attestation.data.target;

        // Attestations must be from the current or previous epoch
        let current_epoch = self.get_current_store_epoch();

        // Use GENESIS_EPOCH for previous when genesis to avoid underflow
        let previous_epoch = if current_epoch > GENESIS_EPOCH {
            current_epoch - 1
        } else {
            GENESIS_EPOCH
        };

        // If attestation target is from a future epoch, delay consideration until the epoch arrives
        ensure!([current_epoch, previous_epoch].contains(&target.epoch));

        Ok(())
    }

    pub fn validate_on_attestation(
        &mut self,
        attestation: &Attestation,
        is_from_block: bool,
    ) -> anyhow::Result<()> {
        let target = attestation.data.target;

        // If the given attestation is not from a beacon block message, we have to check the target
        // epoch scope.
        if !is_from_block {
            self.validate_target_epoch_against_current_time(attestation)?;
        }

        // Check that the epoch number and slot number are matching
        ensure!(target.epoch == compute_epoch_at_slot(attestation.data.slot));

        // Attestation target must be for a known block. If target block is unknown, delay
        // consideration until block is found
        ensure!(self.blocks.contains_key(&target.root));

        // Attestations must be for a known block. If block is unknown, delay consideration until
        // the block is found
        ensure!(
            self.blocks
                .contains_key(&attestation.data.beacon_block_root)
        );
        // Attestations must not be for blocks in the future. If not, the attestation should not be
        // considered
        ensure!(self.blocks[&attestation.data.beacon_block_root].slot <= attestation.data.slot);

        // LMD vote must be consistent with FFG vote target
        ensure!(
            target.root
                == self.get_checkpoint_block(attestation.data.beacon_block_root, target.epoch)?
        );

        // Attestations can only affect the fork choice of subsequent slots.
        // Delay consideration in the fork choice until their slot is in the past.
        ensure!(self.get_current_slot() >= attestation.data.slot + 1);

        Ok(())
    }

    pub fn store_target_checkpoint_state(&mut self, target: Checkpoint) -> anyhow::Result<()> {
        // Store target checkpoint state if not yet seen
        if let Entry::Vacant(entry) = self.checkpoint_states.entry(target) {
            let mut base_state = self.block_states[&target.root].clone();
            let target_slot = compute_start_slot_at_epoch(target.epoch);
            if base_state.slot < target_slot {
                base_state.process_slots(target_slot)?;
            }
            entry.insert(base_state);
        }

        Ok(())
    }

    pub async fn is_data_available(
        &self,
        blob_kzg_commitments: &[KZGCommitment],
        execution_engine: &impl ExecutionApi,
        beacon_block_root: B256,
    ) -> anyhow::Result<bool> {
        // `retrieve_blobs_and_proofs` is implementation and context dependent
        // It returns all the blobs for the given block root, and raises an exception if not
        // available Note: the p2p network does not guarantee sidecar retrieval outside of
        // `MIN_EPOCHS_FOR_BLOB_SIDECARS_REQUESTS`
        let mut blobs_and_proofs: Vec<Option<BlobAndProofV1>> =
            vec![None; blob_kzg_commitments.len()];

        // Try to get blobs_and_proofs from p2p cache
        for (index, blob_and_proof) in blobs_and_proofs.iter_mut().enumerate() {
            *blob_and_proof = self
                .blobs_and_proofs
                .get(&BlobIdentifier::new(beacon_block_root, index as u64))
                .cloned();
        }

        // Fallback to trying engine api
        if blobs_and_proofs.contains(&None) {
            let indexed_blob_versioned_hashes = blobs_and_proofs
                .iter()
                .enumerate()
                .filter_map(|(index, blob_and_proof)| {
                    if blob_and_proof.is_none() {
                        Some((
                            index,
                            blob_kzg_commitments[index].calculate_versioned_hash(),
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let (indices, blob_versioned_hashes): (Vec<_>, Vec<_>) =
                indexed_blob_versioned_hashes.into_iter().unzip();
            let execution_blobs_and_proofs = execution_engine
                .engine_get_blobs_v1(blob_versioned_hashes)
                .await?;
            for (index, blob_and_proof) in indices
                .into_iter()
                .zip(execution_blobs_and_proofs.into_iter())
            {
                blobs_and_proofs[index] = blob_and_proof;
            }
        }

        let blobs_and_proofs = blobs_and_proofs
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow!("Couldn't find all blobs_and_proofs"))?;

        let (blobs, proofs): (Vec<_>, Vec<_>) = blobs_and_proofs
            .into_iter()
            .map(|blob_and_proof| (blob_and_proof.blob, blob_and_proof.proof))
            .unzip();

        ensure!(
            verify_blob_kzg_proof_batch(&blobs, blob_kzg_commitments, &proofs)?,
            "Blob KZG proof batch verification failed (from store)"
        );

        Ok(true)
    }

    pub fn compute_pulled_up_tip(&mut self, block_root: B256) -> anyhow::Result<()> {
        let mut state = self.block_states[&block_root].clone();
        // Pull up the post-state of the block to the next epoch boundary
        state.process_justification_and_finalization()?;

        self.unrealized_justifications
            .insert(block_root, state.current_justified_checkpoint);
        self.update_unrealized_checkpoints(
            state.current_justified_checkpoint,
            state.finalized_checkpoint,
        );

        // If the block is from a prior epoch, apply the realized values
        let block_epoch = compute_epoch_at_slot(self.blocks[&block_root].slot);
        let current_epoch = self.get_current_store_epoch();
        if block_epoch < current_epoch {
            self.update_checkpoints(
                state.current_justified_checkpoint,
                state.finalized_checkpoint,
            );
        }

        Ok(())
    }
}

pub fn get_forkchoice_store(
    anchor_state: BeaconState,
    anchor_block: BeaconBlock,
) -> anyhow::Result<Store> {
    ensure!(anchor_block.state_root == anchor_state.tree_hash_root());
    let anchor_root = anchor_block.tree_hash_root();
    let anchor_epoch = anchor_state.get_current_epoch();
    let justified_checkpoint = Checkpoint {
        epoch: anchor_epoch,
        root: anchor_root,
    };
    let finalized_checkpoint = Checkpoint {
        epoch: anchor_epoch,
        root: anchor_root,
    };
    let proposer_boost_root = B256::ZERO;
    Ok(Store {
        time: anchor_state.genesis_time + SECONDS_PER_SLOT * anchor_state.slot,
        genesis_time: anchor_state.genesis_time,
        justified_checkpoint,
        finalized_checkpoint,
        unrealized_justified_checkpoint: justified_checkpoint,
        unrealized_finalized_checkpoint: finalized_checkpoint,
        proposer_boost_root,
        equivocating_indices: Vec::new(),
        blocks: vec![(anchor_root, anchor_block.clone())]
            .into_iter()
            .collect(),
        block_states: vec![(anchor_root, anchor_state.clone())]
            .into_iter()
            .collect(),
        block_timeliness: HashMap::default(),
        checkpoint_states: vec![(justified_checkpoint, anchor_state.clone())]
            .into_iter()
            .collect(),
        latest_messages: HashMap::default(),
        unrealized_justifications: vec![(anchor_root, justified_checkpoint)]
            .into_iter()
            .collect(),
        blobs_and_proofs: HashMap::default(),
    })
}

pub fn compute_slots_since_epoch_start(slot: u64) -> u64 {
    slot - compute_start_slot_at_epoch(compute_epoch_at_slot(slot))
}
