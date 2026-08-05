use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use alloy_primitives::B256;
use ream_consensus_beacon::data_column_sidecar::ColumnIdentifier;
use ream_consensus_misc::constants::beacon::SLOTS_PER_EPOCH;

/// A pending entry contains untrusted data and is estimated at up to 12.5 MiB on the wire
/// (~10 MiB block + ~2.5 MiB columns), so 160 entries target a 2 GiB retained-data bound.
pub const DEFAULT_MAX_PENDING_ENTRIES: usize = 160;

/// A lookup with no progress for 15 seconds per slot across an epoch can be considered stuck.
/// If no progress is made within this 480-second bound, the lookup is dropped, matching
/// Lighthouse.
pub const DEFAULT_NO_PROGRESS_TIMEOUT: Duration = Duration::from_secs(15 * SLOTS_PER_EPOCH);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLookupConfig {
    pub max_pending_entries: usize,
    pub data_column_retention_epochs: u64,
    pub no_progress_timeout: Duration,
}

impl BlockLookupConfig {
    pub fn for_data_column_retention(retention_epochs: u64) -> Self {
        Self {
            max_pending_entries: DEFAULT_MAX_PENDING_ENTRIES,
            data_column_retention_epochs: retention_epochs,
            no_progress_timeout: DEFAULT_NO_PROGRESS_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingBlockMeta {
    pub block_root: B256,
    pub parent_root: B256,
    pub slot: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingColumnMeta {
    pub identifier: ColumnIdentifier,
    pub slot: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertError {
    Disabled,
    SlotOutsideRetention {
        slot: u64,
        current_slot: u64,
        retention_epochs: u64,
    },
    CapacityUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    Duplicate,
    Rejected(InsertError),
}

#[derive(Debug)]
pub enum CoordinatorAction<BlockPayload, ColumnPayload> {
    /// A pending block that is ready for import because its parent has been imported.
    ImportBlock {
        action_id: ActionId,
        meta: PendingBlockMeta,
        payload: BlockPayload,
    },
    /// A pending data column that is ready for import because its block is pending availability.
    ImportColumn {
        action_id: ActionId,
        meta: PendingColumnMeta,
        payload: ColumnPayload,
    },
}

impl<BlockPayload, ColumnPayload> CoordinatorAction<BlockPayload, ColumnPayload> {
    pub fn action_id(&self) -> ActionId {
        match self {
            Self::ImportBlock { action_id, .. } | Self::ImportColumn { action_id, .. } => {
                *action_id
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionState {
    Waiting,
    Queued,
    InFlight(ActionId),
}

#[derive(Debug)]
struct PendingBlock<BlockPayload> {
    meta: PendingBlockMeta,
    payload: Option<BlockPayload>,
    action_state: ActionState,
}

#[derive(Debug)]
struct PendingColumn<ColumnPayload> {
    meta: PendingColumnMeta,
    payload: Option<ColumnPayload>,
    action_state: ActionState,
}

#[derive(Debug)]
struct PendingBlockEntry<BlockPayload, ColumnPayload> {
    slot: u64,
    last_progress: Instant,
    block: Option<PendingBlock<BlockPayload>>,
    columns: Vec<PendingColumn<ColumnPayload>>,
}

impl<BlockPayload, ColumnPayload> PendingBlockEntry<BlockPayload, ColumnPayload> {
    fn new(slot: u64) -> Self {
        Self {
            slot,
            last_progress: Instant::now(),
            block: None,
            columns: Vec::new(),
        }
    }

    fn has_in_flight_action(&self) -> bool {
        self.block
            .as_ref()
            .is_some_and(|block| matches!(block.action_state, ActionState::InFlight(_)))
            || self
                .columns
                .iter()
                .any(|column| matches!(column.action_state, ActionState::InFlight(_)))
    }

    fn is_empty(&self) -> bool {
        self.block.is_none() && self.columns.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingAction {
    ImportBlock(B256),
    ImportColumn(ColumnIdentifier),
}

/// A bounded dependency graph and work queue for opaque block and column payloads.
/// Validation, lookup, peer and retry policy deliberately live in the caller.
pub struct BlockLookupCoordinator<BlockPayload, ColumnPayload> {
    config: BlockLookupConfig,
    /// Pending blocks and data columns grouped by their owning block root.
    entries: HashMap<B256, PendingBlockEntry<BlockPayload, ColumnPayload>>,
    /// Reverse index from a parent root to its immediate pending child roots.
    children_by_parent: HashMap<B256, VecDeque<B256>>,
    /// FIFO queue of pending items that are ready to be sent to the worker.
    pending_actions: VecDeque<PendingAction>,
    /// Monotonic identifier used to ignore stale worker results.
    next_action_id: u64,
}

impl<BlockPayload, ColumnPayload> BlockLookupCoordinator<BlockPayload, ColumnPayload> {
    pub fn new(config: BlockLookupConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
            children_by_parent: HashMap::new(),
            pending_actions: VecDeque::new(),
            next_action_id: 0,
        }
    }

    /// Holds a validated block until its DA-pending parent is imported.
    pub fn insert_pending_block(
        &mut self,
        meta: PendingBlockMeta,
        payload: BlockPayload,
        current_slot: u64,
    ) -> InsertOutcome {
        if self
            .entries
            .get(&meta.block_root)
            .is_some_and(|entry| entry.block.is_some())
        {
            return InsertOutcome::Duplicate;
        }
        if self.slot_is_outside_retention(meta.slot, current_slot) {
            return InsertOutcome::Rejected(InsertError::SlotOutsideRetention {
                slot: meta.slot,
                current_slot,
                retention_epochs: self.config.data_column_retention_epochs,
            });
        }

        if !self.entries.contains_key(&meta.block_root) {
            if let Err(err) = self.ensure_capacity() {
                return InsertOutcome::Rejected(err);
            }
            self.entries
                .insert(meta.block_root, PendingBlockEntry::new(meta.slot));
        }
        self.children_by_parent
            .entry(meta.parent_root)
            .or_default()
            .push_back(meta.block_root);
        let entry = self
            .entries
            .get_mut(&meta.block_root)
            .expect("pending block entry must exist");
        entry.last_progress = Instant::now();
        entry.block = Some(PendingBlock {
            meta,
            payload: Some(payload),
            action_state: ActionState::Waiting,
        });
        InsertOutcome::Inserted
    }

    /// Holds a validated column until its own block enters the DA checker.
    pub fn insert_pending_column(
        &mut self,
        meta: PendingColumnMeta,
        payload: ColumnPayload,
        current_slot: u64,
    ) -> InsertOutcome {
        let block_root = meta.identifier.block_root;
        if self.entries.get(&block_root).is_some_and(|entry| {
            entry
                .columns
                .iter()
                .any(|column| column.meta.identifier == meta.identifier)
        }) {
            return InsertOutcome::Duplicate;
        }
        if self.slot_is_outside_retention(meta.slot, current_slot) {
            return InsertOutcome::Rejected(InsertError::SlotOutsideRetention {
                slot: meta.slot,
                current_slot,
                retention_epochs: self.config.data_column_retention_epochs,
            });
        }
        if !self.entries.contains_key(&block_root) {
            if let Err(err) = self.ensure_capacity() {
                return InsertOutcome::Rejected(err);
            }
            self.entries
                .insert(block_root, PendingBlockEntry::new(meta.slot));
        }
        let entry = self
            .entries
            .get_mut(&block_root)
            .expect("pending block entry must exist");
        entry.last_progress = Instant::now();
        entry.columns.push(PendingColumn {
            meta,
            payload: Some(payload),
            action_state: ActionState::Waiting,
        });
        InsertOutcome::Inserted
    }

    /// Queues only the immediate pending children of an imported parent.
    pub fn parent_imported(&mut self, parent_root: B256) {
        self.remove_entry(parent_root);
        if let Some(children) = self.children_by_parent.remove(&parent_root) {
            for child_root in children {
                let block = self
                    .entries
                    .get_mut(&child_root)
                    .and_then(|entry| entry.block.as_mut());
                if let Some(block) = block
                    && block.action_state == ActionState::Waiting
                {
                    block.action_state = ActionState::Queued;
                    self.entries
                        .get_mut(&child_root)
                        .expect("queued child entry must exist")
                        .last_progress = Instant::now();
                    self.pending_actions
                        .push_back(PendingAction::ImportBlock(child_root));
                }
            }
        }
    }

    /// Queues a block's own columns only after that block enters the DA checker.
    pub fn mark_block_pending_availability(&mut self, block_root: B256) {
        self.remove_block_only(block_root);
        if let Some(entry) = self.entries.get_mut(&block_root) {
            let mut queued = false;
            for column in &mut entry.columns {
                if column.action_state == ActionState::Waiting {
                    column.action_state = ActionState::Queued;
                    queued = true;
                    self.pending_actions
                        .push_back(PendingAction::ImportColumn(column.meta.identifier));
                }
            }
            if queued {
                entry.last_progress = Instant::now();
            }
        }
    }

    /// Moves one payload to the sequential worker while retaining an in-flight marker.
    pub fn next_action(&mut self) -> Option<CoordinatorAction<BlockPayload, ColumnPayload>> {
        if self.in_flight_action_count() != 0 {
            return None;
        }

        while let Some(pending) = self.pending_actions.pop_front() {
            let action_id = self.next_action_id();
            match pending {
                PendingAction::ImportBlock(block_root) => {
                    let block = self
                        .entries
                        .get_mut(&block_root)
                        .and_then(|entry| entry.block.as_mut());
                    let Some(block) =
                        block.filter(|block| block.action_state == ActionState::Queued)
                    else {
                        continue;
                    };
                    block.action_state = ActionState::InFlight(action_id);
                    return Some(CoordinatorAction::ImportBlock {
                        action_id,
                        meta: block.meta,
                        payload: block
                            .payload
                            .take()
                            .expect("queued block must retain its payload"),
                    });
                }
                PendingAction::ImportColumn(identifier) => {
                    let column = self
                        .entries
                        .get_mut(&identifier.block_root)
                        .and_then(|entry| {
                            entry
                                .columns
                                .iter_mut()
                                .find(|column| column.meta.identifier == identifier)
                        });
                    let Some(column) =
                        column.filter(|column| column.action_state == ActionState::Queued)
                    else {
                        continue;
                    };
                    column.action_state = ActionState::InFlight(action_id);
                    return Some(CoordinatorAction::ImportColumn {
                        action_id,
                        meta: column.meta,
                        payload: column
                            .payload
                            .take()
                            .expect("queued column must retain its payload"),
                    });
                }
            }
        }
        None
    }

    pub fn block_imported(&mut self, action_id: ActionId, block_root: B256) {
        if self.block_action_matches(block_root, action_id) {
            self.parent_imported(block_root);
        }
    }

    pub fn block_pending_availability(&mut self, action_id: ActionId, block_root: B256) {
        if self.block_action_matches(block_root, action_id) {
            self.mark_block_pending_availability(block_root);
        }
    }

    pub fn block_failed(&mut self, action_id: ActionId, block_root: B256) {
        if self.block_action_matches(block_root, action_id) {
            self.remove_entry(block_root);
        }
    }

    pub fn column_finished(&mut self, action_id: ActionId, identifier: ColumnIdentifier) {
        if self.column_action_matches(identifier, action_id) {
            self.remove_column_only(identifier);
            if let Some(entry) = self.entries.get_mut(&identifier.block_root) {
                entry.last_progress = Instant::now();
            }
        }
    }

    /// Drops the sole active action if its worker exits without returning a result.
    pub fn fail_in_flight_action(&mut self) {
        let block_root = self.entries.iter().find_map(|(root, entry)| {
            entry
                .block
                .as_ref()
                .is_some_and(|block| matches!(block.action_state, ActionState::InFlight(_)))
                .then_some(*root)
        });
        if let Some(block_root) = block_root {
            self.remove_entry(block_root);
            return;
        }

        let identifier = self.entries.values().find_map(|entry| {
            entry.columns.iter().find_map(|column| {
                matches!(column.action_state, ActionState::InFlight(_))
                    .then_some(column.meta.identifier)
            })
        });
        if let Some(identifier) = identifier {
            self.remove_column_only(identifier);
        }
    }

    /// Removes non-in-flight entries that finality, DA retention, or lack of progress made stale.
    pub fn prune(&mut self, current_slot: u64, finalized_slot: u64) -> usize {
        let now = Instant::now();
        let expired = self
            .entries
            .iter()
            .filter_map(|(root, entry)| {
                (!entry.has_in_flight_action()
                    && (entry.slot <= finalized_slot
                        || self.slot_is_outside_retention(entry.slot, current_slot)
                        || now.saturating_duration_since(entry.last_progress)
                            >= self.config.no_progress_timeout))
                    .then_some(*root)
            })
            .collect::<Vec<_>>();
        let removed = expired.len();
        for block_root in expired {
            self.remove_entry(block_root);
        }
        removed
    }

    pub fn pending_entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn pending_block_count(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.block.is_some())
            .count()
    }

    pub fn pending_action_count(&self) -> usize {
        self.pending_actions.len()
    }

    pub fn in_flight_action_count(&self) -> usize {
        self.entries
            .values()
            .map(|entry| {
                usize::from(
                    entry.block.as_ref().is_some_and(|block| {
                        matches!(block.action_state, ActionState::InFlight(_))
                    }),
                ) + entry
                    .columns
                    .iter()
                    .filter(|column| matches!(column.action_state, ActionState::InFlight(_)))
                    .count()
            })
            .sum()
    }

    pub fn contains_block(&self, block_root: &B256) -> bool {
        self.entries
            .get(block_root)
            .is_some_and(|entry| entry.block.is_some())
    }

    pub fn contains_column(&self, identifier: &ColumnIdentifier) -> bool {
        self.entries
            .get(&identifier.block_root)
            .is_some_and(|entry| {
                entry
                    .columns
                    .iter()
                    .any(|column| column.meta.identifier == *identifier)
            })
    }

    pub fn children(&self, parent_root: &B256) -> Vec<B256> {
        self.children_by_parent
            .get(parent_root)
            .map(|children| children.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Roots that may need reconciliation after a lagged import broadcast.
    pub fn reconciliation_roots(&self) -> Vec<B256> {
        let mut roots = self.entries.keys().copied().collect::<Vec<_>>();
        roots.extend(self.children_by_parent.keys().copied());
        roots.sort_unstable();
        roots.dedup();
        roots
    }

    fn slot_is_outside_retention(&self, slot: u64, current_slot: u64) -> bool {
        let current_epoch = current_slot / SLOTS_PER_EPOCH;
        let cutoff_epoch = current_epoch.saturating_sub(self.config.data_column_retention_epochs);
        slot < cutoff_epoch.saturating_mul(SLOTS_PER_EPOCH)
    }

    fn ensure_capacity(&mut self) -> Result<(), InsertError> {
        if self.config.max_pending_entries == 0 {
            return Err(InsertError::Disabled);
        }
        if self.entries.len() < self.config.max_pending_entries {
            return Ok(());
        }

        let evicted_root = self
            .entries
            .iter()
            .filter(|(_, entry)| !entry.has_in_flight_action())
            .min_by_key(|(root, entry)| (entry.last_progress, **root))
            .map(|(root, _)| *root)
            .ok_or(InsertError::CapacityUnavailable)?;
        self.remove_entry(evicted_root);
        Ok(())
    }

    fn next_action_id(&mut self) -> ActionId {
        let action_id = ActionId(self.next_action_id);
        self.next_action_id = self.next_action_id.wrapping_add(1);
        action_id
    }

    fn block_action_matches(&self, block_root: B256, action_id: ActionId) -> bool {
        self.entries
            .get(&block_root)
            .and_then(|entry| entry.block.as_ref())
            .is_some_and(|block| block.action_state == ActionState::InFlight(action_id))
    }

    fn column_action_matches(&self, identifier: ColumnIdentifier, action_id: ActionId) -> bool {
        self.entries
            .get(&identifier.block_root)
            .and_then(|entry| {
                entry
                    .columns
                    .iter()
                    .find(|column| column.meta.identifier == identifier)
            })
            .is_some_and(|column| column.action_state == ActionState::InFlight(action_id))
    }

    fn remove_block_only(&mut self, block_root: B256) -> Option<PendingBlock<BlockPayload>> {
        let block = self.entries.get_mut(&block_root)?.block.take()?;
        self.pending_actions.retain(
            |pending| !matches!(pending, PendingAction::ImportBlock(root) if *root == block_root),
        );
        if let Some(children) = self.children_by_parent.get_mut(&block.meta.parent_root) {
            children.retain(|child| *child != block_root);
            if children.is_empty() {
                self.children_by_parent.remove(&block.meta.parent_root);
            }
        }
        if self
            .entries
            .get(&block_root)
            .is_some_and(PendingBlockEntry::is_empty)
        {
            self.entries.remove(&block_root);
        }
        Some(block)
    }

    fn remove_column_only(
        &mut self,
        identifier: ColumnIdentifier,
    ) -> Option<PendingColumn<ColumnPayload>> {
        let entry = self.entries.get_mut(&identifier.block_root)?;
        let position = entry
            .columns
            .iter()
            .position(|column| column.meta.identifier == identifier)?;
        let column = entry.columns.remove(position);
        self.pending_actions.retain(|pending| {
            !matches!(pending, PendingAction::ImportColumn(pending_id) if *pending_id == identifier)
        });
        if entry.is_empty() {
            self.entries.remove(&identifier.block_root);
        }
        Some(column)
    }

    fn remove_entry(&mut self, block_root: B256) {
        let Some(entry) = self.entries.remove(&block_root) else {
            return;
        };
        self.pending_actions.retain(|pending| match pending {
            PendingAction::ImportBlock(root) => *root != block_root,
            PendingAction::ImportColumn(identifier) => identifier.block_root != block_root,
        });
        if let Some(block) = entry.block
            && let Some(children) = self.children_by_parent.get_mut(&block.meta.parent_root)
        {
            children.retain(|child| *child != block_root);
            if children.is_empty() {
                self.children_by_parent.remove(&block.meta.parent_root);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> BlockLookupConfig {
        BlockLookupConfig {
            max_pending_entries: 32,
            data_column_retention_epochs: 2,
            no_progress_timeout: DEFAULT_NO_PROGRESS_TIMEOUT,
        }
    }

    fn block_meta(root: u8, parent: u8, slot: u64) -> PendingBlockMeta {
        PendingBlockMeta {
            block_root: B256::repeat_byte(root),
            parent_root: B256::repeat_byte(parent),
            slot,
        }
    }

    fn column_meta(root: u8, index: u64, slot: u64) -> PendingColumnMeta {
        PendingColumnMeta {
            identifier: ColumnIdentifier::new(B256::repeat_byte(root), index),
            slot,
        }
    }

    #[test]
    fn capacity_evicts_the_oldest_waiting_entry() {
        let mut config = config();
        config.max_pending_entries = 1;
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config);
        assert_eq!(
            coordinator.insert_pending_block(block_meta(1, 9, 1), 1, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(
            coordinator.insert_pending_block(block_meta(2, 9, 1), 2, 1),
            InsertOutcome::Inserted
        );
        assert!(!coordinator.contains_block(&B256::repeat_byte(1)));
        assert!(coordinator.contains_block(&B256::repeat_byte(2)));
    }

    #[test]
    fn capacity_does_not_evict_an_in_flight_entry() {
        let mut config = config();
        config.max_pending_entries = 1;
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config);
        let first = block_meta(1, 9, 1);
        assert_eq!(
            coordinator.insert_pending_block(first, 1, 1),
            InsertOutcome::Inserted
        );
        coordinator.parent_imported(first.parent_root);
        let _action = coordinator.next_action().expect("child should dispatch");

        assert_eq!(
            coordinator.insert_pending_block(block_meta(2, 9, 1), 2, 1),
            InsertOutcome::Rejected(InsertError::CapacityUnavailable)
        );
        assert!(coordinator.contains_block(&first.block_root));
    }

    #[test]
    fn evicting_an_entry_removes_its_queued_actions() {
        let mut config = config();
        config.max_pending_entries = 1;
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config);
        let first = block_meta(1, 9, 1);
        assert_eq!(
            coordinator.insert_pending_block(first, 1, 1),
            InsertOutcome::Inserted
        );
        coordinator.parent_imported(first.parent_root);
        assert_eq!(coordinator.pending_action_count(), 1);

        assert_eq!(
            coordinator.insert_pending_block(block_meta(2, 10, 1), 2, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(coordinator.pending_action_count(), 0);
    }

    #[test]
    fn slots_older_than_da_retention_are_rejected() {
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config());
        assert_eq!(
            coordinator.insert_pending_block(block_meta(1, 9, 31), 1, 3 * SLOTS_PER_EPOCH),
            InsertOutcome::Rejected(InsertError::SlotOutsideRetention {
                slot: 31,
                current_slot: 3 * SLOTS_PER_EPOCH,
                retention_epochs: 2,
            })
        );
    }

    #[test]
    fn block_and_columns_share_one_capacity_entry() {
        let mut config = config();
        config.max_pending_entries = 1;
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config);
        assert_eq!(
            coordinator.insert_pending_block(block_meta(1, 9, 1), 1, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(
            coordinator.insert_pending_column(column_meta(1, 0, 1), 2, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(coordinator.pending_entry_count(), 1);
    }

    #[test]
    fn dispatched_action_keeps_an_in_flight_marker() {
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config());
        let meta = block_meta(1, 9, 1);
        assert_eq!(
            coordinator.insert_pending_block(meta, 7, 1),
            InsertOutcome::Inserted
        );
        coordinator.parent_imported(meta.parent_root);

        assert!(matches!(
            coordinator.next_action(),
            Some(CoordinatorAction::ImportBlock { payload: 7, .. })
        ));
        assert!(coordinator.contains_block(&meta.block_root));
        assert_eq!(coordinator.in_flight_action_count(), 1);
        assert_eq!(
            coordinator.insert_pending_block(meta, 8, 1),
            InsertOutcome::Duplicate
        );
        assert!(coordinator.next_action().is_none());
    }

    #[test]
    fn in_flight_entry_is_not_pruned() {
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config());
        let meta = block_meta(1, 9, 1);
        assert_eq!(
            coordinator.insert_pending_block(meta, 1, 1),
            InsertOutcome::Inserted
        );
        coordinator.parent_imported(meta.parent_root);
        let _action = coordinator.next_action().expect("child should dispatch");

        assert_eq!(coordinator.prune(100, 1), 0);
        assert!(coordinator.contains_block(&meta.block_root));
    }

    #[test]
    fn columns_wait_until_their_block_is_pending_availability() {
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config());
        let meta = column_meta(1, 0, 1);
        assert_eq!(
            coordinator.insert_pending_column(meta, 4, 1),
            InsertOutcome::Inserted
        );
        assert!(coordinator.next_action().is_none());

        coordinator.mark_block_pending_availability(meta.identifier.block_root);
        assert!(matches!(
            coordinator.next_action(),
            Some(CoordinatorAction::ImportColumn { payload: 4, .. })
        ));
    }

    #[test]
    fn pending_releases_columns_but_imported_drops_them() {
        let mut pending = BlockLookupCoordinator::<u8, u8>::new(config());
        let block = block_meta(1, 9, 1);
        let column = column_meta(1, 0, 1);
        assert_eq!(
            pending.insert_pending_block(block, 1, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(
            pending.insert_pending_column(column, 2, 1),
            InsertOutcome::Inserted
        );
        pending.parent_imported(block.parent_root);
        let action_id = pending
            .next_action()
            .expect("block should dispatch")
            .action_id();
        pending.block_pending_availability(action_id, block.block_root);
        assert!(matches!(
            pending.next_action(),
            Some(CoordinatorAction::ImportColumn { payload: 2, .. })
        ));

        let mut imported = BlockLookupCoordinator::<u8, u8>::new(config());
        let block = block_meta(2, 9, 1);
        let column = column_meta(2, 0, 1);
        assert_eq!(
            imported.insert_pending_block(block, 1, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(
            imported.insert_pending_column(column, 2, 1),
            InsertOutcome::Inserted
        );
        imported.parent_imported(block.parent_root);
        let action_id = imported
            .next_action()
            .expect("block should dispatch")
            .action_id();
        imported.block_imported(action_id, block.block_root);
        assert!(!imported.contains_column(&column.identifier));
    }

    #[test]
    fn failed_block_drops_its_columns_but_not_a_sibling() {
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config());
        let failed = block_meta(1, 9, 1);
        let sibling = block_meta(2, 9, 1);
        assert_eq!(
            coordinator.insert_pending_block(failed, 1, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(
            coordinator.insert_pending_block(sibling, 2, 1),
            InsertOutcome::Inserted
        );
        let column = column_meta(1, 0, 1);
        assert_eq!(
            coordinator.insert_pending_column(column, 3, 1),
            InsertOutcome::Inserted
        );

        coordinator.parent_imported(failed.parent_root);
        let action_id = coordinator
            .next_action()
            .expect("failed child should dispatch first")
            .action_id();
        coordinator.block_failed(action_id, failed.block_root);
        assert!(!coordinator.contains_block(&failed.block_root));
        assert!(!coordinator.contains_column(&column.identifier));
        assert!(coordinator.contains_block(&sibling.block_root));
        assert!(matches!(
            coordinator.next_action(),
            Some(CoordinatorAction::ImportBlock { payload: 2, .. })
        ));
    }

    #[test]
    fn stale_worker_result_cannot_remove_a_replacement_entry() {
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config());
        let meta = block_meta(1, 9, 1);
        assert_eq!(
            coordinator.insert_pending_block(meta, 1, 1),
            InsertOutcome::Inserted
        );
        coordinator.parent_imported(meta.parent_root);
        let stale_id = coordinator
            .next_action()
            .expect("block should dispatch")
            .action_id();

        coordinator.parent_imported(meta.block_root);
        assert_eq!(
            coordinator.insert_pending_block(meta, 2, 1),
            InsertOutcome::Inserted
        );
        coordinator.block_failed(stale_id, meta.block_root);
        assert!(coordinator.contains_block(&meta.block_root));
    }

    #[test]
    fn finalized_entries_are_pruned() {
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config());
        assert_eq!(
            coordinator.insert_pending_block(block_meta(1, 9, 1), 1, 1),
            InsertOutcome::Inserted
        );
        assert_eq!(
            coordinator.insert_pending_column(column_meta(2, 0, 4), 2, 4),
            InsertOutcome::Inserted
        );

        assert_eq!(coordinator.prune(4, 1), 1);
        assert_eq!(coordinator.pending_entry_count(), 1);
    }

    #[test]
    fn entries_without_progress_are_pruned() {
        let mut config = config();
        config.no_progress_timeout = Duration::ZERO;
        let mut coordinator = BlockLookupCoordinator::<u8, u8>::new(config);
        assert_eq!(
            coordinator.insert_pending_block(block_meta(1, 9, 1), 1, 1),
            InsertOutcome::Inserted
        );

        assert_eq!(coordinator.prune(1, 0), 1);
    }
}
