use std::{cmp::Ordering, sync::Arc};

use alloy_primitives::B256;
use anyhow::{anyhow, bail, ensure};
use hashbrown::HashMap;
use ream_bls::BLSSignature;
use ream_consensus_beacon::{
    attestation::Attestation,
    data_column_sidecar::ColumnIdentifier,
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_state::BeaconState,
    },
    fork_choice::latest_message::LatestMessage,
    helpers::{calculate_committee_fraction, get_total_active_balance},
};
use ream_consensus_misc::{
    checkpoint::Checkpoint,
    constants::beacon::{
        GENESIS_EPOCH, GENESIS_SLOT, INTERVALS_PER_SLOT,
        MIN_EPOCHS_FOR_DATA_COLUMN_SIDECARS_REQUESTS, SLOTS_PER_EPOCH,
    },
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch, is_shuffling_stable},
};
use ream_data_availability::{DataAvailabilityChecker, PendingBlock};
use ream_network_spec::networks::beacon_network_spec;
use ream_operation_pool::OperationPool;
use ream_storage::{
    db::beacon::BeaconDB,
    tables::{
        field::{CustomField, REDBField},
        multimap_table::MultimapTable,
        table::{CustomTable, REDBTable},
    },
};
use ream_sync_committee_pool::SyncCommitteePool;
use tracing::debug;
use tree_hash::TreeHash;

use crate::constants::{
    PROPOSER_SCORE_BOOST, REORG_HEAD_WEIGHT_THRESHOLD, REORG_MAX_EPOCHS_SINCE_FINALIZATION,
    REORG_PARENT_WEIGHT_THRESHOLD,
};

#[derive(Debug)]
pub struct BlockWithEpochInfo {
    pub block: BeaconBlock,
    pub justified_epoch: u64,
    pub finalized_epoch: u64,
}

#[derive(Debug)]
pub struct Store {
    pub db: BeaconDB,
    pub data_availability_checker: DataAvailabilityChecker,
    pub operation_pool: Arc<OperationPool>,
    pub sync_committee_pool: Arc<SyncCommitteePool>,
}

impl Store {
    pub fn new(
        db: BeaconDB,
        operation_pool: Arc<OperationPool>,
        sync_committee_pool: Option<Arc<SyncCommitteePool>>,
    ) -> Self {
        let sync_committee_pool =
            sync_committee_pool.unwrap_or_else(|| Arc::new(SyncCommitteePool::default()));
        Self {
            db,
            data_availability_checker: DataAvailabilityChecker::supernode(),
            operation_pool,
            sync_committee_pool,
        }
    }

    pub fn backfill_data_availability_columns(
        &mut self,
        block_root: B256,
    ) -> anyhow::Result<Option<PendingBlock>> {
        backfill_data_availability_columns_from_db(
            &self.db,
            &mut self.data_availability_checker,
            block_root,
        )
    }

    pub fn is_previous_epoch_justified(&self) -> anyhow::Result<bool> {
        let current_epoch = self.get_current_store_epoch()?;
        Ok(self.db.justified_checkpoint_provider().get()?.epoch + 1 == current_epoch)
    }

    pub fn get_current_store_epoch(&self) -> anyhow::Result<u64> {
        Ok(compute_epoch_at_slot(self.get_current_slot()?))
    }

    pub fn get_current_slot(&self) -> anyhow::Result<u64> {
        Ok(GENESIS_SLOT + self.get_slots_since_genesis()?)
    }

    pub fn get_slots_since_genesis(&self) -> anyhow::Result<u64> {
        Ok(
            (self.db.time_provider().get()? - self.db.genesis_time_provider().get()?)
                / beacon_network_spec().seconds_per_slot(),
        )
    }

    pub fn get_ancestor(&self, root: B256, slot: u64) -> anyhow::Result<B256> {
        let block = self
            .db
            .block_provider()
            .get(root)?
            .ok_or(anyhow!("Failed to find beacon_block_provider()"))?
            .message;
        if block.slot > slot {
            self.get_ancestor(block.parent_root, slot)
        } else {
            Ok(root)
        }
    }

    /// Compute the checkpoint block for epoch ``epoch`` in the chain of block ``root``
    pub fn get_checkpoint_block(&self, root: B256, epoch: u64) -> anyhow::Result<B256> {
        let epoch_first_slot = compute_start_slot_at_epoch(epoch);
        self.get_ancestor(root, epoch_first_slot)
    }

    pub fn filter_block_tree(
        &self,
        block_root: B256,
        blocks: &mut HashMap<B256, BlockWithEpochInfo>,
    ) -> anyhow::Result<bool> {
        let Some(block) = self.db.block_provider().get(block_root)? else {
            bail!("failed to get block");
        };

        // If any children branches contain expected finalized/justified checkpoints,
        // add to filtered block-tree and signal viability to parent.
        let children = self
            .db
            .parent_root_index_multimap_provider()
            .get(block_root)?
            .unwrap_or_default();

        if !children.is_empty() {
            let filter_results = children
                .iter()
                .map(|child| self.filter_block_tree(*child, blocks))
                .collect::<anyhow::Result<Vec<_>>>()?;

            if filter_results.iter().any(|&result| result) {
                let voting_source = self.get_voting_source(block_root)?;
                let finalized_epoch = self.db.finalized_checkpoint_provider().get()?.epoch;

                blocks.insert(
                    block_root,
                    BlockWithEpochInfo {
                        block: block.message.clone(),
                        // NOTE: Use the node's own `voting_source.epoch` as its `justified_epoch`,
                        // as it means this node justifies the source.
                        justified_epoch: voting_source.epoch,
                        finalized_epoch,
                    },
                );
                return Ok(true);
            }
            return Ok(false);
        }

        let current_epoch = self.get_current_store_epoch()?;
        let voting_source = self.get_voting_source(block_root)?;

        // The voting source should be either at the same height as the store's justified checkpoint
        // or not more than two epochs ago
        let justified_checkpoint_epoch = self.db.justified_checkpoint_provider().get()?.epoch;
        let correct_justified = justified_checkpoint_epoch == GENESIS_EPOCH || {
            voting_source.epoch == justified_checkpoint_epoch
                || voting_source.epoch + 2 >= current_epoch
        };

        let finalized_checkpoint = self.db.finalized_checkpoint_provider().get()?;
        let finalized_checkpoint_block =
            self.get_checkpoint_block(block_root, finalized_checkpoint.epoch)?;

        let correct_finalized = finalized_checkpoint.epoch == GENESIS_EPOCH
            || finalized_checkpoint.root == finalized_checkpoint_block;

        // If expected finalized/justified, add to viable block-tree and signal viability to parent.
        if correct_justified && correct_finalized {
            blocks.insert(
                block_root,
                BlockWithEpochInfo {
                    block: block.message.clone(),
                    // NOTE: Use the node's own `voting_source.epoch` as its `justified_epoch`,
                    // as it means this node justifies the source.
                    justified_epoch: voting_source.epoch,
                    finalized_epoch: finalized_checkpoint.epoch,
                },
            );
            return Ok(true);
        }

        // Otherwise, branch not viable
        Ok(false)
    }

    /// Retrieve a filtered block tree from ``store``, only returning branches
    /// whose leaf state's justified/finalized info agrees with that in ``store``.
    ///
    /// NOTE: ``blocks`` must contain justified/finalized epoch information of its node, so struct
    /// ``BlockWithEpochInfo`` which contains ``justified_epoch`` and ``finalized_epoch`` should
    /// be the value of the map.
    pub fn get_filtered_block_tree(&self) -> anyhow::Result<HashMap<B256, BlockWithEpochInfo>> {
        let base = self.db.justified_checkpoint_provider().get()?.root;
        let mut blocks = HashMap::default();
        self.filter_block_tree(base, &mut blocks)?;
        Ok(blocks)
    }

    pub fn get_head(&self) -> anyhow::Result<B256> {
        // Get filtered block tree that only includes viable branches
        let blocks = self.get_filtered_block_tree()?;
        // Execute the LMD-GHOST fork choice
        let mut head = self.db.justified_checkpoint_provider().get()?.root;

        loop {
            let mut children = vec![];
            for root in blocks.keys() {
                if blocks[root].block.parent_root == head {
                    children.push(root);
                }
            }

            if children.is_empty() {
                return Ok(head);
            }

            let mut weighted_children = children
                .into_iter()
                .map(|child| Ok((*child, self.get_weight(*child)?)))
                .collect::<anyhow::Result<Vec<_>>>()?;

            // Sort by latest attesting balance with ties broken lexicographically
            // Ties broken by favoring block with lexicographically higher root
            weighted_children.sort_by(|(a, weight_a), (b, weight_b)| {
                match weight_a.cmp(weight_b) {
                    Ordering::Equal => a.cmp(b),
                    other => other,
                }
            });

            let Some((best_child, _)) = weighted_children.last() else {
                bail!("Children should always be present");
            };

            head = *best_child;
        }
    }

    /// Update checkpoints in store if necessary
    pub fn update_checkpoints(
        &mut self,
        justified_checkpoint: Checkpoint,
        finalized_checkpoint: Checkpoint,
    ) -> anyhow::Result<()> {
        // Update justified checkpoint
        if justified_checkpoint.epoch > self.db.justified_checkpoint_provider().get()?.epoch {
            self.db
                .justified_checkpoint_provider()
                .insert(justified_checkpoint)?;
        }

        // Update finalized checkpoint
        if finalized_checkpoint.epoch > self.db.finalized_checkpoint_provider().get()?.epoch {
            self.db
                .finalized_checkpoint_provider()
                .insert(finalized_checkpoint)?;
            // Clean operation pool
            if let Some(state) = self.db.state_provider().get(finalized_checkpoint.root)? {
                self.operation_pool.clean_signed_voluntary_exits(&state);

                // Clean expired proposer preparations
                let current_epoch = self.get_current_store_epoch()?;
                self.operation_pool
                    .clean_proposer_preparations(current_epoch);

                if let Some(block) = self.db.block_provider().get(finalized_checkpoint.root)? {
                    for signed_bls_to_execution_change in
                        block.message.body.bls_to_execution_changes
                    {
                        self.operation_pool.remove_signed_bls_to_execution_change(
                            signed_bls_to_execution_change.tree_hash_root(),
                        );
                    }
                }
            }

            // Prune old blobs based on the retention period
            let current_slot = self.get_current_slot()?;
            let min_retention_epochs = beacon_network_spec().min_epochs_for_blob_sidecars_requests;
            match self.db.prune_old_blobs(current_slot, min_retention_epochs) {
                Ok(pruned_count) => {
                    if pruned_count > 0 {
                        tracing::info!("Pruned {} old blobs", pruned_count);
                    }
                }
                Err(err) => {
                    tracing::error!("Failed to prune old blobs: {}", err);
                }
            }
        }

        Ok(())
    }

    /// Update unrealized checkpoints in store if necessary
    pub fn update_unrealized_checkpoints(
        &mut self,
        unrealized_justified_checkpoint: Checkpoint,
        unrealized_finalized_checkpoint: Checkpoint,
    ) -> anyhow::Result<()> {
        // Update unrealized justified checkpoint
        if unrealized_justified_checkpoint.epoch
            > self
                .db
                .unrealized_justified_checkpoint_provider()
                .get()?
                .epoch
        {
            self.db
                .unrealized_justified_checkpoint_provider()
                .insert(unrealized_justified_checkpoint)?;
        }

        // Update unrealized finalized checkpoint
        if unrealized_finalized_checkpoint.epoch
            > self
                .db
                .unrealized_finalized_checkpoint_provider()
                .get()?
                .epoch
        {
            self.db
                .unrealized_finalized_checkpoint_provider()
                .insert(unrealized_finalized_checkpoint)?;
        }

        Ok(())
    }

    // Helper functions
    pub fn is_head_late(&self, head_root: B256) -> anyhow::Result<bool> {
        Ok(!self
            .db
            .block_timeliness_provider()
            .get(head_root)?
            .unwrap_or(true))
    }

    pub fn is_ffg_competitive(&self, head_root: B256, parent_root: B256) -> anyhow::Result<bool> {
        Ok(self
            .db
            .unrealized_justifications_provider()
            .get(head_root)?
            == self
                .db
                .unrealized_justifications_provider()
                .get(parent_root)?)
    }

    pub fn is_proposing_on_time(&self) -> anyhow::Result<bool> {
        // Use half `SECONDS_PER_SLOT // INTERVALS_PER_SLOT` as the proposer reorg deadline
        let time_into_slot = (self.db.time_provider().get()?
            - self.db.genesis_time_provider().get()?)
            % beacon_network_spec().seconds_per_slot();
        let proposer_reorg_cutoff =
            beacon_network_spec().seconds_per_slot() / INTERVALS_PER_SLOT / 2;
        Ok(time_into_slot <= proposer_reorg_cutoff)
    }

    pub fn is_finalization_ok(&self, slot: u64) -> anyhow::Result<bool> {
        let epochs_since_finalization =
            compute_epoch_at_slot(slot) - self.db.finalized_checkpoint_provider().get()?.epoch;
        Ok(epochs_since_finalization <= REORG_MAX_EPOCHS_SINCE_FINALIZATION)
    }

    pub fn get_proposer_score(&self) -> anyhow::Result<u64> {
        let justified_checkpoint_state = self
            .db
            .checkpoint_states_provider()
            .get(self.db.justified_checkpoint_provider().get()?)?
            .ok_or(anyhow!("Failed to find checkpoint in checkpoint states"))?;
        let committee_weight =
            get_total_active_balance(&justified_checkpoint_state) / SLOTS_PER_EPOCH;

        Ok((committee_weight * PROPOSER_SCORE_BOOST) / 100)
    }

    pub fn get_weight(&self, root: B256) -> anyhow::Result<u64> {
        let state = &self
            .db
            .checkpoint_states_provider()
            .get(self.db.justified_checkpoint_provider().get()?)?
            .ok_or_else(|| anyhow!("checkpoint_states not found"))?;

        let unslashed_and_active_indices: Vec<u64> = state
            .get_active_validator_indices(state.get_current_epoch())
            .into_iter()
            .filter(|&i| !state.validators[i as usize].slashed)
            .collect();

        let mut attestation_score: u64 = 0;
        for index in unslashed_and_active_indices {
            if self.db.latest_messages_provider().get(index)?.is_some()
                && !self
                    .db
                    .equivocating_indices_provider()
                    .get()?
                    .contains(&index)
                && self.get_ancestor(
                    self.db
                        .latest_messages_provider()
                        .get(index)?
                        .ok_or_else(|| anyhow!("latest_messages not found"))?
                        .root,
                    self.db
                        .block_provider()
                        .get(root)?
                        .ok_or_else(|| anyhow!(" block not found"))?
                        .message
                        .slot,
                )? == root
            {
                attestation_score += state.validators[index as usize].effective_balance;
            }
        }

        if self.db.proposer_boost_root_provider().get()? == B256::ZERO {
            // Return only attestation score if ``proposer_boost_root`` is not set
            return Ok(attestation_score);
        }

        // Calculate proposer score if ``proposer_boost_root`` is set
        // Boost is applied if ``root`` is an ancestor of ``proposer_boost_root``
        let proposer_score = if self.get_ancestor(
            self.db.proposer_boost_root_provider().get()?,
            self.db
                .block_provider()
                .get(root)?
                .ok_or_else(|| anyhow!("block not found"))?
                .message
                .slot,
        )? == root
        {
            self.get_proposer_score()?
        } else {
            0
        };

        Ok(attestation_score + proposer_score)
    }

    // Compute the voting source checkpoint in event that block with root ``block_root`` is the head
    // block
    pub fn get_voting_source(&self, block_root: B256) -> anyhow::Result<Checkpoint> {
        let block = self
            .db
            .block_provider()
            .get(block_root)?
            .ok_or_else(|| anyhow!("block not found"))?;

        let current_epoch = self.get_current_store_epoch()?;
        let block_epoch = compute_epoch_at_slot(block.message.slot);

        if current_epoch > block_epoch {
            // The block is from a prior epoch, the voting source will be pulled-up
            Ok(self
                .db
                .unrealized_justifications_provider()
                .get(block_root)?
                .ok_or_else(|| anyhow!("unrealized_justifications not found"))?)
        } else {
            // The block is not from a prior epoch, therefore the voting source is not pulled up
            let head_state = self
                .db
                .state_provider()
                .get(block_root)?
                .ok_or_else(|| anyhow!("state not found"))?;
            Ok(head_state.current_justified_checkpoint)
        }
    }

    pub fn is_head_weak(&self, head_root: B256) -> anyhow::Result<bool> {
        let justified_state = self
            .db
            .checkpoint_states_provider()
            .get(self.db.justified_checkpoint_provider().get()?)?
            .ok_or(anyhow!("Justified checkpoint must exist in the store"))?;

        let reorg_threshold =
            calculate_committee_fraction(&justified_state, REORG_HEAD_WEIGHT_THRESHOLD);
        let head_weight = self.get_weight(head_root)?;

        Ok(head_weight < reorg_threshold)
    }

    pub fn is_parent_strong(&self, parent_root: B256) -> anyhow::Result<bool> {
        let justified_state = self
            .db
            .checkpoint_states_provider()
            .get(self.db.justified_checkpoint_provider().get()?)?
            .ok_or(anyhow!("Justified checkpoint must exist in the store"))?;

        let parent_threshold =
            calculate_committee_fraction(&justified_state, REORG_PARENT_WEIGHT_THRESHOLD);
        let parent_weight = self.get_weight(parent_root)?;

        Ok(parent_weight > parent_threshold)
    }

    pub fn get_proposer_head(&self, head_root: B256, slot: u64) -> anyhow::Result<B256> {
        let head_block = self
            .db
            .block_provider()
            .get(head_root)?
            .ok_or(anyhow!("Head block must exist"))?;
        let parent_root = head_block.message.parent_root;
        let parent_block = self
            .db
            .block_provider()
            .get(parent_root)?
            .ok_or(anyhow!("Parent block must exist"))?;

        // Only re-org the head block if it arrived later than the attestation deadline.
        let head_late = self.is_head_late(head_root)?;

        // Do not re-org on an epoch boundary where the proposer shuffling could change.
        let shuffling_stable = is_shuffling_stable(slot);

        // Ensure that the FFG information of the new head will be competitive with the current
        // head.
        let ffg_competitive = self.is_ffg_competitive(head_root, parent_root)?;

        // Do not re-org if the chain is not finalizing with acceptable frequency.
        let finalization_ok = self.is_finalization_ok(slot)?;

        // Only re-org if we are proposing on-time.
        let proposing_on_time = self.is_proposing_on_time()?;

        // Only re-org a single slot at most.
        let parent_slot_ok = parent_block.message.slot + 1 == head_block.message.slot;
        let current_time_ok = head_block.message.slot + 1 == slot;
        let single_slot_reorg = parent_slot_ok && current_time_ok;

        // Check that the head has few enough votes to be overpowered by our proposer boost.
        assert!(self.db.proposer_boost_root_provider().get()? != head_root); // Ensure boost has worn off
        let head_weak = self.is_head_weak(head_root)?;

        // Check that the missing votes are assigned to the parent and not being hoarded.
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
            // We can re-org the current head by building upon its parent block.
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

        let equivocating = self
            .db
            .equivocating_indices_provider()
            .get()
            .unwrap_or_default();

        for &index in &attesting_indices {
            if !equivocating.contains(&index) {
                non_equivocating_attesting_indices.push(index);
            }
        }

        for index in &non_equivocating_attesting_indices {
            if self.db.latest_messages_provider().get(*index)?.is_none()
                || target.epoch
                    > self
                        .db
                        .latest_messages_provider()
                        .get(*index)?
                        .ok_or(anyhow!(
                            "Could not get expected latest message at index: {index}"
                        ))?
                        .epoch
            {
                self.db.latest_messages_provider().insert(
                    *index,
                    LatestMessage {
                        epoch: target.epoch,
                        root: beacon_block_root,
                    },
                )?;
            }
        }

        Ok(())
    }

    pub fn on_tick_per_slot(&mut self, time: u64) -> anyhow::Result<()> {
        let previous_slot = self.get_current_slot()?;

        // Update store time
        self.db.time_provider().insert(time)?;

        let current_slot = self.get_current_slot()?;

        // If this is a new slot, reset store.proposer_boost_root
        if current_slot > previous_slot {
            self.db.proposer_boost_root_provider().insert(B256::ZERO)?;

            // Clean old sync committee messages and contributions per slot
            self.sync_committee_pool
                .clean_sync_committee_messages(current_slot);
            self.sync_committee_pool
                .clean_sync_committee_contributions(current_slot);

            let cutoff_epoch = std::cmp::max(
                self.db.finalized_checkpoint_provider().get()?.epoch + 1,
                self.get_current_store_epoch()?
                    .saturating_sub(MIN_EPOCHS_FOR_DATA_COLUMN_SIDECARS_REQUESTS),
            );
            let cutoff_slot = compute_start_slot_at_epoch(cutoff_epoch);
            let pruned_availability = self.data_availability_checker.prune(cutoff_slot);
            if pruned_availability > 0 {
                debug!("Pruned {pruned_availability} stale pending availability entries");
            }
        }

        // If a new epoch, pull-up justification and finalization from previous epoch
        if current_slot > previous_slot && compute_slots_since_epoch_start(current_slot) == 0 {
            self.update_checkpoints(
                self.db.unrealized_justified_checkpoint_provider().get()?,
                self.db.unrealized_finalized_checkpoint_provider().get()?,
            )?;
        }

        Ok(())
    }

    pub fn validate_target_epoch_against_current_time(
        &mut self,
        attestation: &Attestation,
    ) -> anyhow::Result<()> {
        let target = attestation.data.target;

        // Attestations must be from the current or previous epoch
        let current_epoch = self.get_current_store_epoch()?;

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
        ensure!(self.db.block_provider().get(target.root)?.is_some());

        // Attestations must be for a known block. If block is unknown, delay consideration until
        // the block is found
        ensure!(
            self.db
                .block_provider()
                .get(attestation.data.beacon_block_root)?
                .is_some()
        );
        // Attestations must not be for blocks in the future. If not, the attestation should not be
        // considered
        ensure!(
            self.db
                .block_provider()
                .get(attestation.data.beacon_block_root)?
                .ok_or_else(|| anyhow!("block not found"))?
                .message
                .slot
                <= attestation.data.slot
        );

        // LMD vote must be consistent with FFG vote target
        ensure!(
            target.root
                == self.get_checkpoint_block(attestation.data.beacon_block_root, target.epoch)?
        );

        // Attestations can only affect the fork choice of subsequent slots.
        // Delay consideration in the fork choice until their slot is in the past.
        ensure!(self.get_current_slot()? >= attestation.data.slot + 1);

        Ok(())
    }

    pub fn store_target_checkpoint_state(&mut self, target: Checkpoint) -> anyhow::Result<()> {
        if self.db.checkpoint_states_provider().get(target)?.is_some() {
            return Ok(());
        }

        let Some(mut base_state) = self.db.state_provider().get(target.root)? else {
            return Ok(());
        };

        let target_slot = compute_start_slot_at_epoch(target.epoch);
        if base_state.slot < target_slot {
            base_state.process_slots(target_slot)?;
        }
        self.db
            .checkpoint_states_provider()
            .insert(target, base_state)?;

        Ok(())
    }

    pub fn compute_pulled_up_tip(&mut self, block_root: B256) -> anyhow::Result<()> {
        let mut state = self
            .db
            .state_provider()
            .get(block_root)?
            .ok_or_else(|| anyhow!("beacon state not found"))?;
        // Pull up the post-state of the block to the next epoch boundary
        state.process_justification_and_finalization()?;

        self.db
            .unrealized_justifications_provider()
            .insert(block_root, state.current_justified_checkpoint)?;
        self.update_unrealized_checkpoints(
            state.current_justified_checkpoint,
            state.finalized_checkpoint,
        )?;

        // If the block is from a prior epoch, apply the realized values
        let block_epoch = compute_epoch_at_slot(
            self.db
                .block_provider()
                .get(block_root)?
                .ok_or_else(|| anyhow!("block not found"))?
                .message
                .slot,
        );
        let current_epoch = self.get_current_store_epoch()?;
        if block_epoch < current_epoch {
            self.update_checkpoints(
                state.current_justified_checkpoint,
                state.finalized_checkpoint,
            )?;
        }

        Ok(())
    }

    pub fn is_syncing(&self) -> anyhow::Result<bool> {
        let head = self.get_head()?;

        let head_slot = match self.db.block_provider().get(head) {
            Ok(Some(block)) => block.message.slot,
            err => {
                return Err(anyhow!("Failed to get head slot, error: {err:?}"));
            }
        };

        // calculate sync_distance
        let sync_distance = self.get_current_slot()?.saturating_sub(head_slot);

        Ok(sync_distance > 1)
    }
}

pub fn get_forkchoice_store(
    anchor_state: BeaconState,
    anchor_block: BeaconBlock,
    db: BeaconDB,
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
    let signature = BLSSignature::default();

    let signed_anchor_block = SignedBeaconBlock {
        message: anchor_block,
        signature,
    };

    db.time_provider().insert(
        anchor_state.genesis_time + beacon_network_spec().seconds_per_slot() * anchor_state.slot,
    )?;
    db.genesis_time_provider()
        .insert(anchor_state.genesis_time)?;
    db.justified_checkpoint_provider()
        .insert(justified_checkpoint)?;
    db.finalized_checkpoint_provider()
        .insert(finalized_checkpoint)?;
    db.unrealized_justified_checkpoint_provider()
        .insert(justified_checkpoint)?;
    db.unrealized_finalized_checkpoint_provider()
        .insert(finalized_checkpoint)?;
    db.proposer_boost_root_provider()
        .insert(proposer_boost_root)?;
    db.block_provider()
        .insert(anchor_root, signed_anchor_block)?;
    db.state_provider()
        .insert(anchor_root, anchor_state.clone())?;
    db.state_root_index_provider()
        .insert(anchor_state.tree_hash_root(), anchor_root)?;
    db.slot_index_provider()
        .insert(anchor_state.slot, anchor_root)?;
    db.checkpoint_states_provider()
        .insert(justified_checkpoint, anchor_state)?;
    db.unrealized_justifications_provider()
        .insert(anchor_root, justified_checkpoint)?;

    let operation_pool = Arc::new(OperationPool::default());

    Ok(Store::new(db, operation_pool, None))
}

pub fn compute_slots_since_epoch_start(slot: u64) -> u64 {
    slot - compute_start_slot_at_epoch(compute_epoch_at_slot(slot))
}

fn backfill_data_availability_columns_from_db<State>(
    db: &BeaconDB,
    data_availability_checker: &mut DataAvailabilityChecker<State>,
    block_root: B256,
) -> anyhow::Result<Option<PendingBlock<State>>> {
    let required_columns = data_availability_checker
        .required_columns()
        .iter()
        .copied()
        .collect::<Vec<_>>();

    for column_index in required_columns {
        let column_identifier = ColumnIdentifier::new(block_root, column_index);
        if let Some(sidecar) = db.column_sidecars_provider().get(column_identifier)?
            && let Some(available) = data_availability_checker.add_column(
                block_root,
                column_index,
                sidecar.signed_block_header.message.slot,
            )
        {
            return Ok(Some(available));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use ream_consensus_beacon::{
        data_column_sidecar::DataColumnSidecar,
        electra::{
            beacon_block::{BeaconBlock, SignedBeaconBlock},
            beacon_block_body::BeaconBlockBody,
        },
    };
    use ream_consensus_misc::{
        beacon_block_header::SignedBeaconBlockHeader, constants::beacon::BYTES_PER_COMMITMENT,
        polynomial_commitments::kzg_commitment::KZGCommitment,
    };
    use ream_storage::db::ReamDB;
    use ssz_types::{FixedVector, VariableList};
    use tempdir::TempDir;

    use super::*;

    fn test_db() -> (BeaconDB, TempDir) {
        let temp_dir = TempDir::new("ream_fork_choice_beacon_store").unwrap();
        let db = ReamDB::new(temp_dir.path().to_path_buf())
            .unwrap()
            .init_beacon_db()
            .unwrap();
        (db, temp_dir)
    }

    fn store() -> (Store, TempDir) {
        let (db, temp_dir) = test_db();
        (
            Store::new(db, Arc::new(OperationPool::default()), None),
            temp_dir,
        )
    }

    fn signed_block(slot: u64) -> SignedBeaconBlock {
        SignedBeaconBlock {
            message: BeaconBlock {
                slot,
                body: BeaconBlockBody {
                    blob_kzg_commitments: VariableList::new(vec![KZGCommitment(
                        [0; BYTES_PER_COMMITMENT],
                    )])
                    .unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
            signature: Default::default(),
        }
    }

    fn data_column_sidecar(index: u64, block: &SignedBeaconBlock) -> DataColumnSidecar {
        let mut signed_block_header = SignedBeaconBlockHeader::default();
        signed_block_header.message.slot = block.message.slot;
        signed_block_header.message.proposer_index = block.message.proposer_index;
        signed_block_header.message.parent_root = block.message.parent_root;
        signed_block_header.message.state_root = block.message.state_root;
        signed_block_header.message.body_root = block.message.body.tree_hash_root();
        signed_block_header.signature = block.signature.clone();

        DataColumnSidecar {
            index,
            column: VariableList::empty(),
            kzg_commitments: VariableList::empty(),
            kzg_proofs: VariableList::empty(),
            signed_block_header,
            kzg_commitments_inclusion_proof: FixedVector::default(),
        }
    }

    #[test]
    fn backfill_data_availability_columns_completes_pending_block_from_db() {
        let (db, _temp_dir) = test_db();
        let block = signed_block(11);
        let block_root = block.message.tree_hash_root();
        let sidecar = data_column_sidecar(0, &block);
        let mut checker: DataAvailabilityChecker<()> =
            DataAvailabilityChecker::new(std::collections::HashSet::from([0]));

        db.column_sidecars_provider()
            .insert(ColumnIdentifier::new(block_root, 0), sidecar)
            .unwrap();
        assert!(checker.insert_pending(block_root, block, ()).is_none());

        let pending =
            backfill_data_availability_columns_from_db(&db, &mut checker, block_root).unwrap();

        assert!(pending.is_some());
        assert!(!checker.contains(&block_root));
    }
}
