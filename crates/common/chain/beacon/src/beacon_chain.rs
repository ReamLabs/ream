use std::{collections::VecDeque, sync::Arc};

use alloy_primitives::B256;
use anyhow::bail;
use ream_consensus_beacon::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    electra::beacon_block::SignedBeaconBlock,
};
use ream_consensus_misc::constants::beacon::{FULU_FORK_EPOCH, genesis_validators_root};
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

        let outcome = on_block(
            &mut store,
            &signed_block,
            &self.execution_engine,
            signed_block.message.slot >= beacon_network_spec().slot_n_days_ago(17),
        )
        .await?;

        if outcome == OnBlockOutcome::PendingAvailability {
            debug!(
                "Block is pending data availability: root={}",
                signed_block.message.tree_hash_root()
            );
            return Ok(());
        }

        if outcome == OnBlockOutcome::PendingParent {
            debug!(
                "Block is pending parent import: root={}, parent={}",
                signed_block.message.tree_hash_root(),
                signed_block.message.parent_root
            );
            store.insert_pending_parent_block(signed_block.message.parent_root, signed_block);
            return Ok(());
        }

        self.emit_block_event(&store, &signed_block)?;
        self.process_pending_parent_blocks(&mut store, signed_block.message.tree_hash_root())
            .await?;

        Ok(())
    }

    pub async fn process_data_column_sidecar(
        &self,
        block_root: B256,
        column_index: u64,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;

        if let Some(pending) = store
            .data_availability_checker
            .add_column(block_root, column_index)
        {
            self.import_available_block_and_process_children(&mut store, pending)
                .await?;
        }

        Ok(())
    }

    async fn import_available_block_and_process_children(
        &self,
        store: &mut Store,
        pending: PendingBlock,
    ) -> anyhow::Result<()> {
        let signed_block = pending.signed_block.clone();
        let block_root = signed_block.message.tree_hash_root();
        process_available_block(store, pending)?;
        self.emit_block_event(store, &signed_block)?;
        self.process_pending_parent_blocks(store, block_root).await
    }

    async fn process_pending_parent_blocks(
        &self,
        store: &mut Store,
        parent_root: B256,
    ) -> anyhow::Result<()> {
        let mut blocks = VecDeque::from(store.take_pending_parent_blocks(parent_root));

        while let Some(signed_block) = blocks.pop_front() {
            let outcome = on_block(
                store,
                &signed_block,
                &self.execution_engine,
                signed_block.message.slot >= beacon_network_spec().slot_n_days_ago(17),
            )
            .await?;

            match outcome {
                OnBlockOutcome::Imported => {
                    let block_root = signed_block.message.tree_hash_root();
                    self.emit_block_event(store, &signed_block)?;
                    blocks.extend(store.take_pending_parent_blocks(block_root));
                }
                OnBlockOutcome::PendingAvailability => {
                    debug!(
                        "Pending parent block is now pending data availability: root={}",
                        signed_block.message.tree_hash_root()
                    );
                }
                OnBlockOutcome::PendingParent => {
                    store.insert_pending_parent_block(
                        signed_block.message.parent_root,
                        signed_block,
                    );
                }
            }
        }

        Ok(())
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
            fork_digest: beacon_network_spec()
                .fork_digest(FULU_FORK_EPOCH, genesis_validators_root()),
            finalized_root: finalized_checkpoint.root,
            finalized_epoch: finalized_checkpoint.epoch,
            head_root,
            head_slot,
            earliest_available_slot: 0,
        })
    }
}
