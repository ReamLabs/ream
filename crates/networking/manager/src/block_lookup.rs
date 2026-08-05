use std::{future::Future, sync::Arc};

use alloy_primitives::B256;
use anyhow::{anyhow, bail};
use ream_chain_beacon::beacon_chain::{BeaconChain, BlockImportEvent, BlockProcessingOutcome};
use ream_consensus_beacon::data_column_sidecar::ColumnIdentifier;
use ream_consensus_misc::misc::compute_start_slot_at_epoch;
use ream_fork_choice_beacon::{data_availability::AvailabilityEntryStatus, store::Store};
use ream_storage::tables::{field::REDBField, table::REDBTable};
pub use ream_syncer::block_lookups::{
    ActionId, BlockLookupConfig, CoordinatorAction, InsertError, InsertOutcome, PendingBlockMeta,
    PendingColumnMeta,
};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use tree_hash::TreeHash;

use crate::gossipsub::validate::{
    beacon_block::GossipValidatedBlock, data_column_sidecar::GossipValidatedDataColumn,
};

pub type BlockLookupCoordinator = ream_syncer::block_lookups::BlockLookupCoordinator<
    GossipValidatedBlock,
    GossipValidatedDataColumn,
>;
pub type BlockLookupAction = CoordinatorAction<GossipValidatedBlock, GossipValidatedDataColumn>;

pub fn spawn_block_lookup_worker(
    beacon_chain: Arc<BeaconChain>,
) -> (
    mpsc::Sender<BlockLookupAction>,
    mpsc::Receiver<CoordinatorUpdate>,
) {
    let (action_sender, action_receiver) = mpsc::channel::<BlockLookupAction>(1);
    let (update_sender, update_receiver) = mpsc::channel(1);
    spawn_sequential_worker(action_receiver, update_sender, move |action| {
        let beacon_chain = beacon_chain.clone();
        async move { execute_coordinator_action(action, &beacon_chain).await }
    });
    (action_sender, update_receiver)
}

fn spawn_sequential_worker<Action, Update, Execute, ExecuteFuture>(
    mut action_receiver: mpsc::Receiver<Action>,
    update_sender: mpsc::Sender<Update>,
    mut execute: Execute,
) where
    Action: Send + 'static,
    Update: Send + 'static,
    Execute: FnMut(Action) -> ExecuteFuture + Send + 'static,
    ExecuteFuture: Future<Output = Update> + Send + 'static,
{
    tokio::spawn(async move {
        while let Some(action) = action_receiver.recv().await {
            let update = execute(action).await;
            if update_sender.send(update).await.is_err() {
                break;
            }
        }
    });
}

pub enum PendingGossipItem {
    Block { block: GossipValidatedBlock },
    Column { column: GossipValidatedDataColumn },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorUpdate {
    /// The pending block was imported successfully.
    BlockImported {
        action_id: ActionId,
        block_root: B256,
    },
    /// The pending block passed block processing but is waiting for data availability.
    BlockPendingAvailability {
        action_id: ActionId,
        block_root: B256,
    },
    /// The pending block could not be imported and will be dropped.
    BlockFailed {
        action_id: ActionId,
        block_root: B256,
    },
    /// The pending data column import attempt finished, whether it succeeded or failed.
    ColumnFinished {
        action_id: ActionId,
        identifier: ColumnIdentifier,
    },
}

pub fn insert_pending_item(
    coordinator: &mut BlockLookupCoordinator,
    item: PendingGossipItem,
    current_slot: u64,
) -> InsertOutcome {
    match item {
        PendingGossipItem::Block { block } => {
            let signed_block = block.block();
            let meta = PendingBlockMeta {
                block_root: signed_block.message.tree_hash_root(),
                parent_root: signed_block.message.parent_root,
                slot: signed_block.message.slot,
            };
            coordinator.insert_pending_block(meta, block, current_slot)
        }
        PendingGossipItem::Column { column } => {
            let sidecar = column.sidecar();
            let block_root = sidecar.signed_block_header.message.tree_hash_root();
            let meta = PendingColumnMeta {
                identifier: ColumnIdentifier::new(block_root, sidecar.index),
                slot: sidecar.signed_block_header.message.slot,
            };
            coordinator.insert_pending_column(meta, column, current_slot)
        }
    }
}

pub fn apply_coordinator_update(
    coordinator: &mut BlockLookupCoordinator,
    update: CoordinatorUpdate,
) {
    match update {
        CoordinatorUpdate::BlockImported {
            action_id,
            block_root,
        } => {
            // The worker caused this import, so advance the graph directly. The broadcast remains
            // necessary for imports from range sync/RPC and for imports completed by columns.
            coordinator.block_imported(action_id, block_root);
        }
        CoordinatorUpdate::BlockPendingAvailability {
            action_id,
            block_root,
        } => {
            coordinator.block_pending_availability(action_id, block_root);
        }
        CoordinatorUpdate::BlockFailed {
            action_id,
            block_root,
        } => {
            coordinator.block_failed(action_id, block_root);
        }
        CoordinatorUpdate::ColumnFinished {
            action_id,
            identifier,
        } => {
            coordinator.column_finished(action_id, identifier);
        }
    }
}

pub fn apply_block_import_event<BlockPayload, ColumnPayload>(
    coordinator: &mut ream_syncer::block_lookups::BlockLookupCoordinator<
        BlockPayload,
        ColumnPayload,
    >,
    event: BlockImportEvent,
) {
    match event {
        BlockImportEvent::Imported { block_root } => coordinator.parent_imported(block_root),
        BlockImportEvent::PendingAvailability { block_root } => {
            coordinator.mark_block_pending_availability(block_root);
        }
    }
}

pub async fn execute_coordinator_action(
    action: BlockLookupAction,
    beacon_chain: &BeaconChain,
) -> CoordinatorUpdate {
    match action {
        CoordinatorAction::ImportBlock {
            action_id,
            meta,
            payload,
            ..
        } => {
            if let Err(err) =
                ensure_pending_item_is_importable(beacon_chain, meta.slot, meta.parent_root, None)
                    .await
            {
                debug!(
                    block_root = ?meta.block_root,
                    ?err,
                    "Dropping pending block whose release conditions no longer hold"
                );
                return CoordinatorUpdate::BlockFailed {
                    action_id,
                    block_root: meta.block_root,
                };
            }

            // Import directly because revalidating this gossip-validated block would self-ignore.
            match beacon_chain.process_block(payload.into_inner()).await {
                Ok(BlockProcessingOutcome::Imported { block_root }) => {
                    CoordinatorUpdate::BlockImported {
                        action_id,
                        block_root,
                    }
                }
                Ok(BlockProcessingOutcome::PendingAvailability { block_root }) => {
                    CoordinatorUpdate::BlockPendingAvailability {
                        action_id,
                        block_root,
                    }
                }
                Err(err) => {
                    warn!(
                        block_root = ?meta.block_root,
                        ?err,
                        "Failed to import validated pending block"
                    );
                    CoordinatorUpdate::BlockFailed {
                        action_id,
                        block_root: meta.block_root,
                    }
                }
            }
        }
        CoordinatorAction::ImportColumn {
            action_id,
            meta,
            payload,
            ..
        } => {
            let sidecar = payload.sidecar();
            let parent_root = sidecar.signed_block_header.message.parent_root;
            let data_column_block_root = meta.identifier.block_root;
            let import_result = beacon_chain
                .import_data_column_sidecar_if(payload.into_inner(), move |store| {
                    ensure_pending_item_is_importable_with_store(
                        store,
                        meta.slot,
                        parent_root,
                        Some(data_column_block_root),
                    )
                })
                .await;
            if let Err(err) = import_result {
                warn!(
                    ?meta.identifier,
                    ?err,
                    "Failed to import validated pending data column"
                );
            }
            CoordinatorUpdate::ColumnFinished {
                action_id,
                identifier: meta.identifier,
            }
        }
    }
}

async fn ensure_pending_item_is_importable(
    beacon_chain: &BeaconChain,
    slot: u64,
    parent_root: B256,
    data_column_block_root: Option<B256>,
) -> anyhow::Result<()> {
    let store = beacon_chain.store.lock().await;
    ensure_pending_item_is_importable_with_store(&store, slot, parent_root, data_column_block_root)
}

fn ensure_pending_item_is_importable_with_store(
    store: &Store,
    slot: u64,
    parent_root: B256,
    data_column_block_root: Option<B256>,
) -> anyhow::Result<()> {
    let current_slot = store.get_current_slot()?;
    let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
    let finalized_slot = compute_start_slot_at_epoch(finalized_checkpoint.epoch);

    ensure_slot_and_parent_are_importable(
        slot,
        current_slot,
        finalized_slot,
        store.db.block_provider().get(parent_root)?.is_some(),
        store.db.state_provider().get(parent_root)?.is_some(),
    )?;

    #[cfg(not(feature = "disable_ancestor_validation"))]
    if store.get_checkpoint_block(parent_root, finalized_checkpoint.epoch)?
        != finalized_checkpoint.root
    {
        bail!("finalized checkpoint is no longer an ancestor");
    }

    // Make sure data column only's imported when it's actually in DA checker module
    if let Some(block_root) = data_column_block_root
        && !matches!(
            store.data_availability_checker.status(&block_root),
            AvailabilityEntryStatus::PendingBlock | AvailabilityEntryStatus::Complete
        )
    {
        bail!("column's block is no longer pending data availability");
    }

    Ok(())
}

/// Making sure block is importable if it's in range (finalized_checkpoint_head, current head], and
/// parent's state exist
fn ensure_slot_and_parent_are_importable(
    slot: u64,
    current_slot: u64,
    finalized_slot: u64,
    parent_block_known: bool,
    parent_state_known: bool,
) -> anyhow::Result<()> {
    if slot > current_slot {
        bail!("item is from a future slot");
    }
    if slot <= finalized_slot {
        bail!("finality advanced past the pending item");
    }
    if !parent_block_known || !parent_state_known {
        return Err(anyhow!("parent block or state is not imported"));
    }
    Ok(())
}

pub async fn import_validated_data_column(
    beacon_chain: &BeaconChain,
    sidecar: ream_consensus_beacon::data_column_sidecar::DataColumnSidecar,
) -> anyhow::Result<()> {
    beacon_chain
        .import_data_column_sidecar_if(sidecar, |_| Ok(()))
        .await
}

pub fn log_insert_outcome(block_root: B256, outcome: InsertOutcome) {
    match outcome {
        InsertOutcome::Inserted => {}
        InsertOutcome::Duplicate => {
            debug!(?block_root, "Ignored duplicate pending block lookup data");
        }
        InsertOutcome::Rejected(reason) => {
            warn!(?block_root, ?reason, "Rejected pending block insertion");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use tokio::{sync::Notify, time::timeout};

    use super::*;

    #[test]
    fn pending_item_import_rejects_future_and_finalized_items() {
        assert!(ensure_slot_and_parent_are_importable(11, 10, 5, true, true).is_err());
        assert!(ensure_slot_and_parent_are_importable(5, 10, 5, true, true).is_err());
        assert!(ensure_slot_and_parent_are_importable(4, 10, 5, true, true).is_err());
    }

    #[test]
    fn pending_item_import_requires_imported_parent_block_and_state() {
        assert!(ensure_slot_and_parent_are_importable(6, 10, 5, false, true).is_err());
        assert!(ensure_slot_and_parent_are_importable(6, 10, 5, true, false).is_err());
        assert!(ensure_slot_and_parent_are_importable(6, 10, 5, true, true).is_ok());
    }

    #[test]
    fn external_pending_availability_event_releases_its_pending_columns() {
        ream_network_spec::networks::beacon::initialize_test_network_spec();
        let mut coordinator = ream_syncer::block_lookups::BlockLookupCoordinator::<u8, u8>::new(
            BlockLookupConfig::for_data_column_retention(1),
        );
        let block_root = B256::repeat_byte(1);
        let identifier = ColumnIdentifier::new(block_root, 0);
        assert!(matches!(
            coordinator.insert_pending_column(
                PendingColumnMeta {
                    identifier,
                    slot: 1,
                },
                2,
                1,
            ),
            InsertOutcome::Inserted
        ));

        apply_block_import_event(
            &mut coordinator,
            BlockImportEvent::PendingAvailability { block_root },
        );

        assert_eq!(coordinator.pending_block_count(), 0);
        assert!(matches!(
            coordinator.next_action(),
            Some(CoordinatorAction::ImportColumn { meta, payload: 2, .. })
                if meta.identifier == identifier
        ));
    }

    #[tokio::test]
    async fn manager_work_can_progress_while_worker_import_is_in_flight() {
        let (action_sender, action_receiver) = mpsc::channel(1);
        let (update_sender, mut update_receiver) = mpsc::channel(1);
        let import_started = Arc::new(Notify::new());
        let release_import = Arc::new(Notify::new());
        let started = import_started.clone();
        let release = release_import.clone();
        spawn_sequential_worker(action_receiver, update_sender, move |action| {
            let started = started.clone();
            let release = release.clone();
            async move {
                started.notify_one();
                release.notified().await;
                action
            }
        });

        action_sender
            .send(7_u8)
            .await
            .expect("worker should be open");
        timeout(Duration::from_secs(1), import_started.notified())
            .await
            .expect("worker should start the import");

        let (manager_sender, mut manager_receiver) = mpsc::channel(1);
        manager_sender
            .send("gossip")
            .await
            .expect("manager event channel should be open");
        assert_eq!(
            timeout(Duration::from_millis(50), async {
                tokio::select! {
                    event = manager_receiver.recv() => event,
                    update = update_receiver.recv() => {
                        panic!("worker unexpectedly completed before manager input: {update:?}")
                    }
                }
            })
            .await
            .expect("the manager select must remain pollable while import executes"),
            Some("gossip")
        );
        assert!(update_receiver.try_recv().is_err());

        release_import.notify_one();
        assert_eq!(
            timeout(Duration::from_secs(1), update_receiver.recv())
                .await
                .expect("worker result should arrive"),
            Some(7)
        );
    }
}
