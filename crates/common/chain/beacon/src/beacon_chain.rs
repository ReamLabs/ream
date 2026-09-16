use std::sync::Arc;

use alloy_primitives::B256;
use anyhow::{anyhow, bail};
use ream_consensus_beacon::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    data_column_sidecar::NUMBER_OF_COLUMNS, electra::beacon_block::SignedBeaconBlock,
};
use ream_consensus_misc::{
    constants::beacon::{MAX_BLOBS_PER_BLOCK_ELECTRA, genesis_validators_root},
    misc::compute_epoch_at_slot,
};
use ream_events_beacon::{BeaconEvent, BeaconEventSender, event::chain::BlockEvent};
use ream_execution_engine::ExecutionEngine;
use ream_execution_rpc_types::forkchoice_update::ForkchoiceStateV1;
use ream_fork_choice_beacon::{
    handlers::{on_attestation, on_attester_slashing, on_block, on_tick},
    store::Store,
};
use ream_metrics::{BEACON_HEAD_EPOCH, BEACON_HEAD_SLOT, BEACON_REORGS_TOTAL};
use ream_network_spec::networks::beacon_network_spec;
use ream_operation_pool::OperationPool;
use ream_req_resp::beacon::messages::status::Status;
use ream_storage::{
    db::beacon::BeaconDB,
    tables::{
        field::REDBField,
        table::{CustomTable, REDBTable},
    },
};
use ream_sync_committee_pool::SyncCommitteePool;
use tokio::sync::{Mutex, broadcast};
use tracing::{debug, info, warn};
use tree_hash::TreeHash;

/// BeaconChain is the main struct which manages the nodes local beacon chain.
pub struct BeaconChain {
    pub store: Mutex<Store>,
    pub execution_engine: Option<ExecutionEngine>,
    pub event_sender: Option<broadcast::Sender<BeaconEvent>>,
}

impl BeaconChain {
    /// Creates a new instance of `BeaconChain`.
    pub fn new(
        db: BeaconDB,
        operation_pool: Arc<OperationPool>,
        sync_committee_pool: Arc<SyncCommitteePool>,
        execution_engine: Option<ExecutionEngine>,
        event_sender: Option<broadcast::Sender<BeaconEvent>>,
    ) -> Self {
        Self {
            store: Mutex::new(Store::new(db, operation_pool, Some(sync_committee_pool))),
            execution_engine,
            event_sender,
        }
    }

    pub async fn process_block(&self, signed_block: SignedBeaconBlock) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        let previous_head = store.get_head().ok();

        let result = on_block(
            &mut store,
            &signed_block,
            &self.execution_engine,
            signed_block.message.slot >= beacon_network_spec().slot_n_days_ago(17),
            false, // full validation
        )
        .await;

        if let Err(e) = result {
            let err_str = e.to_string();
            if err_str.starts_with("INVALID_PAYLOAD") {
                let block_root = signed_block.message.tree_hash_root();
                let latest_valid = if let Some((_, hash_str)) = err_str.split_once(':') {
                    hash_str
                        .parse::<alloy_primitives::B256>()
                        .unwrap_or(B256::ZERO)
                } else {
                    B256::ZERO
                };
                self.handle_invalid_payload(store, block_root, latest_valid)
                    .await?;
                bail!("Block payload is invalid");
            }
            return Err(e);
        }

        for attestation in signed_block.message.body.attestations.iter() {
            if let Err(err) = on_attestation(&mut store, attestation.clone(), true) {
                warn!("Failed to process block attestation through fork choice: {err:?}");
            }
        }
        update_head_metrics_and_reorg(&store, previous_head);

        // Build and Emit Block event
        let finalized_checkpoint = store.db.finalized_checkpoint_provider().get().ok();
        let block_event =
            BlockEvent::from_block(&signed_block, finalized_checkpoint, |block_root, epoch| {
                store.get_checkpoint_block(block_root, epoch)
            })?;
        self.event_sender
            .send_event(BeaconEvent::Block(block_event));

        Ok(())
    }

    pub async fn process_block_optimistic(
        &self,
        signed_block: SignedBeaconBlock,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        let previous_head = store.get_head().ok();

        on_block(
            &mut store,
            &signed_block,
            &self.execution_engine,
            signed_block.message.slot >= beacon_network_spec().slot_n_days_ago(17),
            true, // skip execution validation
        )
        .await?;

        // Insert the root as optimistic in the database
        let block_root = signed_block.message.tree_hash_root();
        store
            .db
            .optimistic_roots_provider()
            .insert(block_root, true)?;

        for attestation in signed_block.message.body.attestations.iter() {
            if let Err(err) = on_attestation(&mut store, attestation.clone(), true) {
                warn!("Failed to process block attestation through fork choice: {err:?}");
            }
        }
        update_head_metrics_and_reorg(&store, previous_head);

        // Build and Emit Block event
        let finalized_checkpoint = store.db.finalized_checkpoint_provider().get().ok();
        let block_event =
            BlockEvent::from_block(&signed_block, finalized_checkpoint, |block_root, epoch| {
                store.get_checkpoint_block(block_root, epoch)
            })?;
        self.event_sender
            .send_event(BeaconEvent::Block(block_event));

        Ok(())
    }

    pub async fn process_attester_slashing(
        &self,
        attester_slashing: AttesterSlashing,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_attester_slashing(&mut store, attester_slashing)?;
        Ok(())
    }

    pub async fn handle_invalid_payload(
        &self,
        store: tokio::sync::MutexGuard<'_, Store>,
        invalid_root: B256,
        latest_valid_hash: B256,
    ) -> anyhow::Result<()> {
        info!(
            "Handling invalid payload: invalid_root={:?}, latest_valid_hash={:?}",
            invalid_root, latest_valid_hash
        );

        // Safety guard: if the invalid block was rejected before being imported into the store,
        // and latest_valid_hash is either not provided or already matches the canonical head,
        // the existing chain in the store is unaffected and does not need rollback.
        let is_imported = store.db.block_provider().get(invalid_root)?.is_some();
        if !is_imported {
            if latest_valid_hash == B256::ZERO {
                debug!(
                    "Invalid block {:?} was never imported into store; skipping rollback",
                    invalid_root
                );
                return Ok(());
            }

            if let Ok(head_root) = store.get_head()
                && let Ok(Some(head_block)) = store.db.block_provider().get(head_root)
                && (head_root == latest_valid_hash
                    || head_block.message.body.execution_payload.block_hash == latest_valid_hash)
            {
                debug!(
                    "Invalid block {:?} was not imported and current head {:?} matches latest_valid_hash; skipping rollback",
                    invalid_root, head_root
                );
                return Ok(());
            }
        }

        // Collect (block_root, parent_root, slot) for all invalid blocks so we can
        // clean up all index tables without calling store.get_head() after removal.
        let mut to_remove: Vec<(B256, B256, u64)> = vec![];
        let mut last_valid_root = B256::ZERO;
        let mut current = store.get_head()?;
        let mut found_target = false;

        loop {
            let block = store
                .db
                .block_provider()
                .get(current)?
                .ok_or_else(|| anyhow!("Missing block for root {current:?}"))?;

            let parent_root = block.message.parent_root;
            let slot = block.message.slot;

            if latest_valid_hash != B256::ZERO
                && (current == latest_valid_hash
                    || block.message.body.execution_payload.block_hash == latest_valid_hash)
            {
                // current block is the latest valid — it stays, record it as new head
                last_valid_root = current;
                found_target = true;
                break;
            }

            to_remove.push((current, parent_root, slot));

            if current == invalid_root {
                // Parent of invalid_root becomes the new head
                last_valid_root = parent_root;
                found_target = true;
                break;
            }

            current = parent_root;
            if current == B256::ZERO {
                break;
            }
        }

        // Safety guard: if we traversed all the way to genesis without finding invalid_root or
        // latest_valid_hash, the head chain does not contain the invalid block. Do NOT delete to_remove!
        if !found_target {
            warn!(
                "Neither invalid_root {:?} nor latest_valid_hash {:?} was found in head chain; aborting rollback to prevent data loss",
                invalid_root, latest_valid_hash
            );
            return Ok(());
        }

        for (root, parent_root, slot) in to_remove {
            debug!("Removing invalid block and components for root {:?}", root);

            // Remove from all DB tables so filter_block_tree() stays consistent
            store.db.block_provider().remove(root)?;
            store.db.state_provider().remove(root)?;
            let _ = store.db.slot_index_provider().remove(slot);
            let _ = store
                .db
                .parent_root_index_multimap_provider()
                .remove_child(parent_root, root);
            let _ = store.db.unrealized_justifications_provider().remove(root);
            let _ = store.db.optimistic_roots_provider().remove(root);

            for index in 0..MAX_BLOBS_PER_BLOCK_ELECTRA {
                let blob_id = ream_consensus_beacon::blob_sidecar::BlobIdentifier::new(root, index);
                let _ = store.db.blobs_and_proofs_provider().remove(blob_id);
            }

            for index in 0..NUMBER_OF_COLUMNS {
                let col_id =
                    ream_consensus_beacon::data_column_sidecar::ColumnIdentifier::new(root, index);
                let _ = store.db.column_sidecars_provider().remove(col_id);
            }
        }

        // Use last_valid_root computed during traversal — avoids calling store.get_head()
        // after removal, which would fail if the removed block is still referenced in
        // filter_block_tree's multimap traversal.
        let new_head = last_valid_root;
        info!("New head after invalid payload rollback: {:?}", new_head);

        // If we couldn't determine a valid head, skip EL notification
        if new_head == B256::ZERO {
            drop(store);
            return Ok(());
        }

        let new_head_block =
            store.db.block_provider().get(new_head)?.ok_or_else(|| {
                anyhow!("New head block not found after rollback: {new_head:?}")
            })?;
        let head_block_hash = new_head_block.message.body.execution_payload.block_hash;

        let justified_checkpoint = store.db.justified_checkpoint_provider().get()?;
        let safe_block_hash = if justified_checkpoint.root == B256::ZERO {
            B256::ZERO
        } else {
            store
                .db
                .block_provider()
                .get(justified_checkpoint.root)?
                .map(|b| b.message.body.execution_payload.block_hash)
                .unwrap_or(B256::ZERO)
        };

        let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
        let finalized_block_hash = if finalized_checkpoint.root == B256::ZERO {
            B256::ZERO
        } else {
            store
                .db
                .block_provider()
                .get(finalized_checkpoint.root)?
                .map(|b| b.message.body.execution_payload.block_hash)
                .unwrap_or(B256::ZERO)
        };

        let forkchoice_state = ForkchoiceStateV1 {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        };

        drop(store);

        if let Some(ref execution_engine) = self.execution_engine {
            execution_engine
                .engine_forkchoice_updated_v3(forkchoice_state, None)
                .await?;
        }

        Ok(())
    }

    pub async fn process_attestation(
        &self,
        attestation: Attestation,
        is_from_block: bool,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_attestation(&mut store, attestation, is_from_block)?;
        Ok(())
    }

    pub async fn process_tick(&self, time: u64) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_tick(&mut store, time)?;
        Ok(())
    }

    pub async fn build_status_request(&self) -> anyhow::Result<Status> {
        let Ok(finalized_checkpoint) = self
            .store
            .lock()
            .await
            .db
            .finalized_checkpoint_provider()
            .get()
        else {
            bail!("Failed to get finalized checkpoint");
        };

        let head_root = match self.store.lock().await.get_head() {
            Ok(head) => head,
            Err(err) => {
                warn!("Failed to get head root: {err}, falling back to finalized root");
                finalized_checkpoint.root
            }
        };

        let head_slot = match self.store.lock().await.db.block_provider().get(head_root) {
            Ok(Some(block)) => block.message.slot,
            err => {
                bail!("Failed to get block for head root {head_root}: {err:?}");
            }
        };

        Ok(Status {
            fork_digest: beacon_network_spec().fork_digest(
                beacon_network_spec().current_epoch(),
                genesis_validators_root(),
            ),
            finalized_root: finalized_checkpoint.root,
            finalized_epoch: finalized_checkpoint.epoch,
            head_root,
            head_slot,
            earliest_available_slot: 0,
        })
    }
}

fn update_head_metrics_and_reorg(store: &Store, previous_head: Option<B256>) {
    match store.get_head() {
        Ok(new_head) => {
            match store.db.block_provider().get(new_head) {
                Ok(Some(new_head_block)) => {
                    let new_head_slot = new_head_block.message.slot;
                    BEACON_HEAD_SLOT.set(new_head_slot as i64);
                    BEACON_HEAD_EPOCH.set(compute_epoch_at_slot(new_head_slot) as i64);
                }
                Ok(None) => {
                    warn!(
                        "head block {new_head:?} not found in store; skipping head metrics update"
                    );
                }
                Err(err) => {
                    warn!("Failed to fetch head block for metrics: {err:?}");
                }
            }

            // Detect canonical chain reorgs for beacon_reorgs_total.
            if let Some(previous_head) = previous_head
                && previous_head != new_head
            {
                match store.db.block_provider().get(previous_head) {
                    Ok(Some(previous_head_block)) => {
                        let previous_head_slot = previous_head_block.message.slot;
                        match store.get_ancestor(new_head, previous_head_slot) {
                            Ok(ancestor) => {
                                if ancestor != previous_head {
                                    BEACON_REORGS_TOTAL.inc();
                                }
                            }
                            Err(err) => {
                                warn!("Failed to check ancestor for reorg detection: {err:?}");
                            }
                        }
                    }
                    Ok(None) => {
                        warn!(
                            "previous head block {previous_head:?} not found in store; skipping reorg check"
                        );
                    }
                    Err(err) => {
                        warn!("Failed to fetch previous head block for reorg check: {err:?}");
                    }
                }
            }
        }
        Err(err) => {
            warn!("Failed to get head for metrics/reorg detection: {err:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{B256, aliases::B32};
    use ream_consensus_beacon::{
        blob_sidecar::BlobIdentifier,
        data_column_sidecar::{ColumnIdentifier, DataColumnSidecar},
        electra::{
            beacon_block::{BeaconBlock, SignedBeaconBlock},
            beacon_block_body::BeaconBlockBody,
            beacon_state::BeaconState,
        },
    };
    use ream_consensus_misc::checkpoint::Checkpoint;
    use ream_network_spec::networks::beacon::initialize_test_network_spec;
    use ream_storage::{db::ReamDB, tables::multimap_table::MultimapTable};
    use ssz_types::VariableList;
    use tempdir::TempDir;
    use tree_hash::TreeHash;

    fn create_dummy_block(
        slot: u64,
        parent_root: B256,
        exec_block_hash: B256,
    ) -> SignedBeaconBlock {
        let mut signed = SignedBeaconBlock {
            message: BeaconBlock {
                slot,
                proposer_index: 0,
                parent_root,
                state_root: B256::ZERO,
                body: BeaconBlockBody::default(),
            },
            signature: Default::default(),
        };
        signed.message.body.execution_payload.block_hash = exec_block_hash;
        signed
    }

    fn create_dummy_column_sidecar() -> DataColumnSidecar {
        DataColumnSidecar {
            index: 0,
            column: VariableList::empty(),
            kzg_commitments: VariableList::empty(),
            kzg_proofs: VariableList::empty(),
            signed_block_header: Default::default(),
            kzg_commitments_inclusion_proof: Default::default(),
        }
    }

    fn create_dummy_state() -> BeaconState {
        BeaconState {
            genesis_time: 0,
            genesis_validators_root: B256::ZERO,
            slot: 0,
            fork: ream_consensus_misc::fork::Fork {
                previous_version: B32::ZERO,
                current_version: B32::ZERO,
                epoch: 0,
            },
            latest_block_header: Default::default(),
            block_roots: Default::default(),
            state_roots: Default::default(),
            historical_roots: Default::default(),
            eth1_data: Default::default(),
            eth1_data_votes: Default::default(),
            eth1_deposit_index: 0,
            validators: Default::default(),
            balances: Default::default(),
            randao_mixes: Default::default(),
            slashings: Default::default(),
            previous_epoch_participation: Default::default(),
            current_epoch_participation: Default::default(),
            justification_bits: Default::default(),
            previous_justified_checkpoint: Default::default(),
            current_justified_checkpoint: Default::default(),
            finalized_checkpoint: Default::default(),
            inactivity_scores: Default::default(),
            current_sync_committee: Arc::new(
                ream_consensus_beacon::sync_committee::SyncCommittee {
                    public_keys: Default::default(),
                    aggregate_public_key: Default::default(),
                },
            ),
            next_sync_committee: Arc::new(ream_consensus_beacon::sync_committee::SyncCommittee {
                public_keys: Default::default(),
                aggregate_public_key: Default::default(),
            }),
            latest_execution_payload_header: Default::default(),
            next_withdrawal_index: 0,
            next_withdrawal_validator_index: 0,
            historical_summaries: Default::default(),
            deposit_requests_start_index: 0,
            deposit_balance_to_consume: 0,
            exit_balance_to_consume: 0,
            earliest_exit_epoch: 0,
            consolidation_balance_to_consume: 0,
            earliest_consolidation_epoch: 0,
            pending_deposits: Default::default(),
            pending_partial_withdrawals: Default::default(),
            pending_consolidations: Default::default(),
            proposer_lookahead: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_handle_invalid_payload_cleanup() -> anyhow::Result<()> {
        println!("[TEST] Starting test_handle_invalid_payload_cleanup");
        initialize_test_network_spec();
        println!("[TEST] Network spec initialized");
        let tmp = TempDir::new("beacon_chain_test")?;
        let ream_db = ReamDB::new(tmp.path().to_path_buf())?;
        let db = ream_db.init_beacon_db()?;
        println!("[TEST] DB initialized");

        // Setup base / anchor block
        let mut anchor_state = create_dummy_state();
        anchor_state.genesis_time = 1000;
        anchor_state.slot = 0;
        println!("[TEST] Computing anchor state tree hash root...");
        let state_root = anchor_state.tree_hash_root();
        println!("[TEST] Anchor state tree hash root: {:?}", state_root);
        let anchor_block = BeaconBlock {
            slot: 0,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root,
            body: Default::default(),
        };
        println!("[TEST] Computing anchor block tree hash root...");
        let anchor_root = anchor_block.tree_hash_root();
        println!("[TEST] Anchor block tree hash root: {:?}", anchor_root);

        println!("[TEST] Creating forkchoice store...");
        let store = ream_fork_choice_beacon::store::get_forkchoice_store(
            anchor_state,
            anchor_block,
            db.clone(),
        )?;
        println!("[TEST] Forkchoice store created");

        let chain = BeaconChain {
            store: tokio::sync::Mutex::new(store),
            execution_engine: None,
            event_sender: None,
        };

        // Insert some blocks building on top of anchor_root
        // Block 1 (valid block)
        let b1 = create_dummy_block(1, anchor_root, B256::from([11u8; 32]));
        let b1_root = b1.message.tree_hash_root();

        // Block 2 (valid block)
        let b2 = create_dummy_block(2, b1_root, B256::from([22u8; 32]));
        let b2_root = b2.message.tree_hash_root();

        // Block 3 (invalid block)
        let b3 = create_dummy_block(3, b2_root, B256::from([33u8; 32]));
        let b3_root = b3.message.tree_hash_root();
        println!("[TEST] Blocks created");

        // Add blocks to block_provider and multimap
        {
            println!("[TEST] Locking store to insert mock data...");
            let store_lock = chain.store.lock().await;
            println!("[TEST] Store locked, inserting blocks...");
            store_lock.db.block_provider().insert(b1_root, b1.clone())?;
            store_lock.db.block_provider().insert(b2_root, b2.clone())?;
            store_lock.db.block_provider().insert(b3_root, b3.clone())?;

            store_lock
                .db
                .state_provider()
                .insert(b1_root, create_dummy_state())?;
            store_lock
                .db
                .state_provider()
                .insert(b2_root, create_dummy_state())?;
            store_lock
                .db
                .state_provider()
                .insert(b3_root, create_dummy_state())?;

            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(anchor_root, b1_root)?;
            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(b1_root, b2_root)?;
            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(b2_root, b3_root)?;

            store_lock.db.slot_index_provider().insert(1, b1_root)?;
            store_lock.db.slot_index_provider().insert(2, b2_root)?;
            store_lock.db.slot_index_provider().insert(3, b3_root)?;

            // Insert blobs and columns to ensure they are cleaned up
            let blob_id_b3 = BlobIdentifier::new(b3_root, 0);
            store_lock
                .db
                .blobs_and_proofs_provider()
                .insert(blob_id_b3, Default::default())?;

            let col_id_b3 = ColumnIdentifier::new(b3_root, 0);
            store_lock
                .db
                .column_sidecars_provider()
                .insert(col_id_b3, create_dummy_column_sidecar())?;

            store_lock.db.unrealized_justifications_provider().insert(
                b1_root,
                Checkpoint {
                    epoch: 0,
                    root: anchor_root,
                },
            )?;
            store_lock.db.unrealized_justifications_provider().insert(
                b2_root,
                Checkpoint {
                    epoch: 0,
                    root: anchor_root,
                },
            )?;
            store_lock.db.unrealized_justifications_provider().insert(
                b3_root,
                Checkpoint {
                    epoch: 0,
                    root: anchor_root,
                },
            )?;
            println!("[TEST] Mock data inserted");
        }

        // Verify that store.get_head() is b3_root
        {
            println!("[TEST] Verifying get_head is b3_root...");
            let store_lock = chain.store.lock().await;
            let head = store_lock.get_head()?;
            println!("[TEST] Head is: {:?}", head);
            assert_eq!(head, b3_root);
        }

        // Call handle_invalid_payload where b3 is invalid, and b2 is the latest valid hash (execution hash 22)
        println!("[TEST] Calling handle_invalid_payload...");
        let store_guard = chain.store.lock().await;
        chain
            .handle_invalid_payload(store_guard, b3_root, B256::from([22u8; 32]))
            .await?;
        println!("[TEST] handle_invalid_payload returned!");

        // Verify that b3 was removed, along with state, blobs and columns, and b2 is now the head
        let store_lock = chain.store.lock().await;
        println!("[TEST] Verifying new head is b2_root...");
        let new_head = store_lock.get_head()?;
        println!("[TEST] New head is: {:?}", new_head);
        assert_eq!(new_head, b2_root);
        assert!(store_lock.db.block_provider().get(b3_root)?.is_none());
        assert!(store_lock.db.state_provider().get(b3_root)?.is_none());
        assert!(
            store_lock
                .db
                .blobs_and_proofs_provider()
                .get(BlobIdentifier::new(b3_root, 0))?
                .is_none()
        );
        assert!(
            store_lock
                .db
                .column_sidecars_provider()
                .get(ColumnIdentifier::new(b3_root, 0))?
                .is_none()
        );

        // Verify b2 and b1 still exist
        assert!(store_lock.db.block_provider().get(b2_root)?.is_some());
        assert!(store_lock.db.block_provider().get(b1_root)?.is_some());
        println!("[TEST] Test completed successfully!");

        Ok(())
    }

    /// Test: optimistic block root is stored in the optimistic_roots table when
    /// `optimistic_roots_provider().insert()` is called (as done by process_block_optimistic).
    #[tokio::test]
    async fn test_optimistic_root_storage() -> anyhow::Result<()> {
        initialize_test_network_spec();
        let tmp = TempDir::new("beacon_chain_test_opt")?;
        let ream_db = ReamDB::new(tmp.path().to_path_buf())?;
        let db = ream_db.init_beacon_db()?;

        let anchor_state = create_dummy_state();
        let state_root = anchor_state.tree_hash_root();
        let anchor_block = BeaconBlock {
            slot: 0,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root,
            body: Default::default(),
        };
        let anchor_root = anchor_block.tree_hash_root();

        let store = ream_fork_choice_beacon::store::get_forkchoice_store(
            anchor_state,
            anchor_block,
            db.clone(),
        )?;

        let chain = BeaconChain {
            store: tokio::sync::Mutex::new(store),
            execution_engine: None,
            event_sender: None,
        };

        // Insert a block and mark it as optimistic
        let b1 = create_dummy_block(1, anchor_root, B256::from([11u8; 32]));
        let b1_root = b1.message.tree_hash_root();
        {
            let store_lock = chain.store.lock().await;
            store_lock.db.block_provider().insert(b1_root, b1.clone())?;
            store_lock
                .db
                .state_provider()
                .insert(b1_root, create_dummy_state())?;
            // Simulate what process_block_optimistic does: insert root into optimistic table
            store_lock
                .db
                .optimistic_roots_provider()
                .insert(b1_root, true)?;
        }

        // Verify the optimistic root is stored
        {
            let store_lock = chain.store.lock().await;
            let is_optimistic = store_lock.db.optimistic_roots_provider().get(b1_root)?;
            assert_eq!(
                is_optimistic,
                Some(true),
                "Block should be marked as optimistic"
            );
        }

        // Verify it can be removed (simulating validation)
        {
            let store_lock = chain.store.lock().await;
            store_lock.db.optimistic_roots_provider().remove(b1_root)?;
            let is_optimistic = store_lock.db.optimistic_roots_provider().get(b1_root)?;
            assert!(
                is_optimistic.is_none(),
                "Optimistic root should be removed after validation"
            );
        }

        Ok(())
    }

    /// Test: invalid payload with multiple descendants — A→B→C→D, C invalid.
    /// Expected: C and D removed, B is the new head, A still exists.
    #[tokio::test]
    async fn test_invalid_payload_multi_descendant_rollback() -> anyhow::Result<()> {
        initialize_test_network_spec();
        let tmp = TempDir::new("beacon_chain_test_multi")?;
        let ream_db = ReamDB::new(tmp.path().to_path_buf())?;
        let db = ream_db.init_beacon_db()?;

        let mut anchor_state = create_dummy_state();
        anchor_state.genesis_time = 1000;
        anchor_state.slot = 0;
        let state_root = anchor_state.tree_hash_root();
        let anchor_block = BeaconBlock {
            slot: 0,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root,
            body: Default::default(),
        };
        let anchor_root = anchor_block.tree_hash_root();

        let store = ream_fork_choice_beacon::store::get_forkchoice_store(
            anchor_state,
            anchor_block,
            db.clone(),
        )?;

        let chain = BeaconChain {
            store: tokio::sync::Mutex::new(store),
            execution_engine: None,
            event_sender: None,
        };

        // Chain: anchor → b1 (valid) → b2 (valid) → b3 (invalid) → b4 (descendant of invalid)
        let b1 = create_dummy_block(1, anchor_root, B256::from([11u8; 32]));
        let b1_root = b1.message.tree_hash_root();
        let b2 = create_dummy_block(2, b1_root, B256::from([22u8; 32]));
        let b2_root = b2.message.tree_hash_root();
        let b3 = create_dummy_block(3, b2_root, B256::from([33u8; 32]));
        let b3_root = b3.message.tree_hash_root();
        let b4 = create_dummy_block(4, b3_root, B256::from([44u8; 32]));
        let b4_root = b4.message.tree_hash_root();

        {
            let store_lock = chain.store.lock().await;

            for (root, block) in [
                (b1_root, b1.clone()),
                (b2_root, b2.clone()),
                (b3_root, b3.clone()),
                (b4_root, b4.clone()),
            ] {
                store_lock.db.block_provider().insert(root, block)?;
                store_lock
                    .db
                    .state_provider()
                    .insert(root, create_dummy_state())?;
                store_lock.db.unrealized_justifications_provider().insert(
                    root,
                    Checkpoint {
                        epoch: 0,
                        root: anchor_root,
                    },
                )?;
            }

            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(anchor_root, b1_root)?;
            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(b1_root, b2_root)?;
            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(b2_root, b3_root)?;
            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(b3_root, b4_root)?;

            store_lock.db.slot_index_provider().insert(1, b1_root)?;
            store_lock.db.slot_index_provider().insert(2, b2_root)?;
            store_lock.db.slot_index_provider().insert(3, b3_root)?;
            store_lock.db.slot_index_provider().insert(4, b4_root)?;

            // Mark b3 and b4 as optimistic
            store_lock
                .db
                .optimistic_roots_provider()
                .insert(b3_root, true)?;
            store_lock
                .db
                .optimistic_roots_provider()
                .insert(b4_root, true)?;
        }

        // Head should be b4
        {
            let store_lock = chain.store.lock().await;
            assert_eq!(
                store_lock.get_head()?,
                b4_root,
                "Head should be b4 before rollback"
            );
        }

        // b3 invalid, latest valid = b2 (exec hash [22u8;32])
        let store_guard = chain.store.lock().await;
        chain
            .handle_invalid_payload(store_guard, b3_root, B256::from([22u8; 32]))
            .await?;

        // Verify b3 and b4 removed, b2 is head, b1 still present
        let store_lock = chain.store.lock().await;
        let new_head = store_lock.get_head()?;
        assert_eq!(new_head, b2_root, "Head should revert to b2 after rollback");

        assert!(
            store_lock.db.block_provider().get(b3_root)?.is_none(),
            "b3 should be removed"
        );
        assert!(
            store_lock.db.state_provider().get(b3_root)?.is_none(),
            "b3 state should be removed"
        );
        assert!(
            store_lock
                .db
                .optimistic_roots_provider()
                .get(b3_root)?
                .is_none(),
            "b3 optimistic root should be cleaned"
        );

        // Note: b4 removal depends on handle_invalid_payload traversal from head downward.
        // The current impl traverses from head to invalid_root, so b4 (head) is removed first.
        assert!(
            store_lock.db.block_provider().get(b4_root)?.is_none(),
            "b4 descendant should be removed"
        );
        assert!(
            store_lock
                .db
                .optimistic_roots_provider()
                .get(b4_root)?
                .is_none(),
            "b4 optimistic root should be cleaned"
        );

        assert!(
            store_lock.db.block_provider().get(b2_root)?.is_some(),
            "b2 should remain"
        );
        assert!(
            store_lock.db.block_provider().get(b1_root)?.is_some(),
            "b1 should remain"
        );

        Ok(())
    }

    /// Test: when an invalid block was never imported into the store (e.g. failed in on_block),
    /// handle_invalid_payload must NOT wipe the existing head or canonical chain.
    #[tokio::test]
    async fn test_handle_invalid_payload_unimported_block_preserves_head() -> anyhow::Result<()> {
        initialize_test_network_spec();
        let tmp = TempDir::new("beacon_chain_test_unimported")?;
        let ream_db = ReamDB::new(tmp.path().to_path_buf())?;
        let db = ream_db.init_beacon_db()?;

        let mut anchor_state = create_dummy_state();
        anchor_state.genesis_time = 1000;
        anchor_state.slot = 0;
        let state_root = anchor_state.tree_hash_root();
        let anchor_block = BeaconBlock {
            slot: 0,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root,
            body: Default::default(),
        };
        let anchor_root = anchor_block.tree_hash_root();

        let store = ream_fork_choice_beacon::store::get_forkchoice_store(
            anchor_state,
            anchor_block,
            db.clone(),
        )?;

        let chain = BeaconChain {
            store: tokio::sync::Mutex::new(store),
            execution_engine: None,
            event_sender: None,
        };

        // Valid block 1
        let b1 = create_dummy_block(1, anchor_root, B256::from([11u8; 32]));
        let b1_root = b1.message.tree_hash_root();

        {
            let store_lock = chain.store.lock().await;
            store_lock.db.block_provider().insert(b1_root, b1.clone())?;
            store_lock
                .db
                .state_provider()
                .insert(b1_root, create_dummy_state())?;
            store_lock
                .db
                .parent_root_index_multimap_provider()
                .insert(anchor_root, b1_root)?;
            store_lock.db.slot_index_provider().insert(1, b1_root)?;
            store_lock.db.unrealized_justifications_provider().insert(
                b1_root,
                Checkpoint {
                    epoch: 0,
                    root: anchor_root,
                },
            )?;
        }

        // Head is b1
        {
            let store_lock = chain.store.lock().await;
            assert_eq!(store_lock.get_head()?, b1_root);
        }

        // An invalid block that was rejected during on_block (never added to block_provider)
        let unimported_invalid_root = B256::from([99u8; 32]);

        // Calling handle_invalid_payload should safely return without deleting b1 or anchor
        let store_guard = chain.store.lock().await;
        chain
            .handle_invalid_payload(store_guard, unimported_invalid_root, B256::ZERO)
            .await?;

        // Head and blocks must still exist intact
        let store_lock = chain.store.lock().await;
        assert_eq!(store_lock.get_head()?, b1_root);
        assert!(store_lock.db.block_provider().get(b1_root)?.is_some());
        assert!(store_lock.db.block_provider().get(anchor_root)?.is_some());

        Ok(())
    }
}
