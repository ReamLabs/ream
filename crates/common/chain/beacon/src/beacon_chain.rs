use std::sync::Arc;

use alloy_primitives::B256;
use anyhow::bail;
use ream_consensus_beacon::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    electra::beacon_block::SignedBeaconBlock,
};
use ream_consensus_misc::{
    constants::beacon::genesis_validators_root, misc::compute_epoch_at_slot,
};
use ream_events_beacon::{BeaconEvent, BeaconEventSender, event::chain::BlockEvent};
use ream_execution_engine::ExecutionEngine;
use ream_fork_choice_beacon::{
    data_availability::PendingBlock,
    handlers::{
        OnBlockOutcome, on_attestation, on_attester_slashing, on_block, on_tick,
        process_available_block,
    },
    store::Store,
};
use ream_metrics::{BEACON_HEAD_EPOCH, BEACON_HEAD_SLOT, BEACON_REORGS_TOTAL};
use ream_network_spec::networks::beacon_network_spec;
use ream_operation_pool::OperationPool;
use ream_req_resp::beacon::messages::status::Status;
use ream_storage::{
    db::beacon::BeaconDB,
    tables::{field::REDBField, table::REDBTable},
};
use ream_sync_committee_pool::SyncCommitteePool;
use tokio::sync::{Mutex, broadcast};
use tracing::{debug, warn};
use tree_hash::TreeHash;

/// BeaconChain is the main struct which manages the nodes local beacon chain.
pub struct BeaconChain {
    pub store: Mutex<Store>,
    pub execution_engine: Option<ExecutionEngine>,
    pub event_sender: Option<broadcast::Sender<BeaconEvent>>,
    force_data_availability_checks: bool,
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
            force_data_availability_checks: false,
        }
    }

    /// Enables data availability checks independently of the configured Fulu fork epoch.
    /// Intended for test networks that exercise Fulu data flow on an Electra state fixture.
    pub fn force_data_availability_checks(mut self) -> Self {
        self.force_data_availability_checks = true;
        self
    }

    pub async fn process_block(&self, signed_block: SignedBeaconBlock) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        let previous_head = store.get_head().ok();
        let network_spec = beacon_network_spec();
        let verify_data_availability = self.force_data_availability_checks
            || is_data_availability_check_required(
                compute_epoch_at_slot(signed_block.message.slot),
                store.get_current_store_epoch()?,
                network_spec.fulu_fork_epoch,
                network_spec.min_epochs_for_data_column_sidecars_requests,
            );

        let outcome = on_block(
            &mut store,
            &signed_block,
            &self.execution_engine,
            verify_data_availability,
        )
        .await?;

        if outcome == OnBlockOutcome::PendingAvailability {
            debug!(
                "Block is pending data availability: root={}",
                signed_block.message.tree_hash_root()
            );
            return Ok(());
        }

        self.process_block_attestations(&mut store, &signed_block);
        self.update_head_metrics(&store, previous_head);
        self.emit_block_event(&store, &signed_block)?;

        Ok(())
    }

    pub async fn process_data_column_sidecar(
        &self,
        block_root: B256,
        column_index: u64,
        slot: u64,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;

        // Block with available data columns will be stored here, this is
        // a guard check to prevent processing a column for an imported block
        if store.db.block_provider().get(block_root)?.is_some() {
            return Ok(());
        }

        if let Some(pending) =
            store
                .data_availability_checker
                .add_column(block_root, column_index, slot)
        {
            self.import_available_block(&mut store, pending)?;
        }

        Ok(())
    }

    fn import_available_block(
        &self,
        store: &mut Store,
        pending: PendingBlock,
    ) -> anyhow::Result<()> {
        let previous_head = store.get_head().ok();
        let signed_block = pending.signed_block.clone();
        process_available_block(store, pending)?;
        self.process_block_attestations(store, &signed_block);
        self.update_head_metrics(store, previous_head);
        self.emit_block_event(store, &signed_block)
    }

    fn process_block_attestations(&self, store: &mut Store, signed_block: &SignedBeaconBlock) {
        for attestation in signed_block.message.body.attestations.iter() {
            if let Err(err) = on_attestation(store, attestation.clone(), true) {
                warn!("Failed to process block attestation through fork choice: {err:?}");
            }
        }
    }

    /// Refreshes head/reorg metrics after a block is imported. Called from both the direct
    /// import path and the column-completion path, since either can move the head.
    fn update_head_metrics(&self, store: &Store, previous_head: Option<B256>) {
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

    fn emit_block_event(
        &self,
        store: &Store,
        signed_block: &SignedBeaconBlock,
    ) -> anyhow::Result<()> {
        let finalized_checkpoint = store.db.finalized_checkpoint_provider().get().ok();
        let block_event =
            BlockEvent::from_block(signed_block, finalized_checkpoint, |block_root, epoch| {
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

// Check data availability only for blocks within the sidecar retention window.
// Sidecars for blocks older than roughly 18 days may no longer be available.
fn is_data_availability_check_required(
    block_epoch: u64,
    current_epoch: u64,
    fulu_fork_epoch: u64,
    retention_epochs: u64,
) -> bool {
    let boundary_epoch = std::cmp::max(
        fulu_fork_epoch,
        current_epoch.saturating_sub(retention_epochs),
    );

    block_epoch >= boundary_epoch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_availability_boundary_tracks_fulu_and_retention_window() {
        let fulu_epoch = 10;
        let retention_epochs = 100;
        assert!(!is_data_availability_check_required(
            9,
            10,
            fulu_epoch,
            retention_epochs,
        ));
        assert!(is_data_availability_check_required(
            10,
            10,
            fulu_epoch,
            retention_epochs,
        ));

        let current_epoch = fulu_epoch + retention_epochs + 10;
        assert!(!is_data_availability_check_required(
            fulu_epoch + 9,
            current_epoch,
            fulu_epoch,
            retention_epochs,
        ));
        assert!(is_data_availability_check_required(
            fulu_epoch + 10,
            current_epoch,
            fulu_epoch,
            retention_epochs,
        ));
    }
}
