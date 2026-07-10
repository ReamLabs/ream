use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        RwLock, RwLockReadGuard, RwLockWriteGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use alloy_primitives::B256;
use ream_data_availability::{
    availability::ColumnAvailability,
    column::{ColumnContext, VerifiedColumn},
    error::ColumnStoreError,
    id::{ALL_COLUMNS_MASK, ColumnId, NUMBER_OF_COLUMNS, column_indices},
    store::{ColumnReadStore, ColumnWriteStore, InsertOutcome},
};
use tracing::{debug, info, trace, warn};

/// Committed column files; in-progress writes use `.tmp` instead.
const COLUMN_FILE_EXTENSION: &str = "ssz";

/// Kept in `root` next to the column files. The name never matches the
/// column-file scheme, so the startup scan skips it.
const RETENTION_FLOOR_FILE: &str = "retention_floor";

/// A block's slot plus a 128-bit bitmap of its stored column indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlockEntry {
    slot: u64,
    columns: u128,
}

/// `column_index < NUMBER_OF_COLUMNS` for any valid id, so the shift never
/// exceeds the width of `u128`.
fn column_bit(column_index: u64) -> u128 {
    debug_assert!(column_index < NUMBER_OF_COLUMNS);
    1u128 << column_index
}

/// File-backed column store: one file per verified column under `root`, plus an
/// in-memory metadata index (block root → [`BlockEntry`]) rebuilt from the
/// directory on startup.
pub struct FileColumnStore {
    root: PathBuf,
    index: RwLock<HashMap<B256, BlockEntry>>,
    // TODO: add a payload cache to avoid reading from files every time.
    /// Beacon-issued retention floor, as a slot; `0` means no floor yet.
    retention_floor: AtomicU64,
}

impl FileColumnStore {
    /// Open a store rooted at `root`, rebuilding the index from files already
    /// on disk. A missing directory yields an empty store; leftover `*.tmp`
    /// files from an interrupted write are removed.
    pub fn new(root: PathBuf) -> Result<Self, ColumnStoreError> {
        // The floor must be known before the directory scan, so the scan can
        // apply it to the column files it finds.
        let retention_floor = Self::load_retention_floor(&root);
        let store = Self {
            root,
            index: RwLock::new(HashMap::new()),
            retention_floor: AtomicU64::new(retention_floor),
        };
        store.rebuild_index()?;
        Ok(store)
    }

    /// Read the persisted retention floor; `0` (no floor) when the file does
    /// not exist yet.
    fn load_retention_floor(root: &Path) -> u64 {
        let path = root.join(RETENTION_FLOOR_FILE);
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return 0,
            Err(err) => {
                warn!(
                    "failed to read the retention floor file {}: {err}; starting with no floor",
                    path.display()
                );
                return 0;
            }
        };
        match contents.trim().parse() {
            Ok(floor) => floor,
            Err(err) => {
                warn!(
                    "malformed retention floor file {}: {err}; starting with no floor",
                    path.display()
                );
                0
            }
        }
    }

    /// Durably record `floor`: temp file + `sync_all` + atomic rename, the same
    /// discipline as a column write.
    fn persist_retention_floor(&self, floor: u64) -> Result<(), ColumnStoreError> {
        // The first retention hint may arrive before any column was written.
        fs::create_dir_all(&self.root)?;
        let path = self.root.join(RETENTION_FLOOR_FILE);
        let tmp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(floor.to_string().as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// `{slot:08}_{index:03}_{block_root:x}.ssz` under `root`.
    fn column_path(&self, id: &ColumnId, slot: u64) -> PathBuf {
        let block_root = id.block_root();
        let index = id.index();
        self.root.join(format!(
            "{slot:08}_{index:03}_{block_root:x}.{COLUMN_FILE_EXTENSION}"
        ))
    }

    fn rebuild_index(&self) -> Result<(), ColumnStoreError> {
        // make sure that old data will be deleted and won't be loaded
        self.prune_below_slot(self.get_retention_floor());

        let root = self.root.display();
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            // No directory yet: nothing has been stored, which is normal.
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                debug!("no column store directory at {root} yet; starting with an empty index");
                return Ok(());
            }
            Err(err) => return Err(err.into()),
        };

        let mut index = self.index_write();
        let mut column_files = 0u64;
        for entry in entries {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            // A half-written file from a crashed `put` is never trustworthy.
            if path.extension().and_then(|ext| ext.to_str()) == Some("tmp") {
                debug!(
                    "removing leftover temp file from an interrupted write: {}",
                    path.display()
                );
                let _ = fs::remove_file(&path);
                continue;
            }

            // Skip anything we did not write: startup must not fail on clutter.
            if let Some((id, slot)) = Self::parse_column_file_name(name) {
                index
                    .entry(id.block_root())
                    .or_insert(BlockEntry { slot, columns: 0 })
                    .columns |= column_bit(id.index());
                column_files += 1;
            }
        }

        info!(
            "loaded DA store index from {root}: {column_files} column files across {} blocks, retention floor at slot {}",
            index.len(),
            self.retention_floor.load(Ordering::Relaxed)
        );
        Ok(())
    }

    /// Inverse of [`FileColumnStore::column_path`]; `None` for any file name we
    /// never emit.
    fn parse_column_file_name(name: &str) -> Option<(ColumnId, u64)> {
        let stem = name.strip_suffix(&format!(".{COLUMN_FILE_EXTENSION}"))?;
        let mut parts = stem.split('_');
        let slot = parts.next()?.parse::<u64>().ok()?;
        let index = parts.next()?.parse::<u64>().ok()?;
        let block_root = B256::from_str(parts.next()?).ok()?;
        if parts.next().is_some() {
            return None;
        }
        let id = ColumnId::new(block_root, index).ok()?;
        Some((id, slot))
    }

    fn remove_entries(&self, entries: &[(B256, BlockEntry)]) -> Result<usize, ColumnStoreError> {
        let mut removed = 0;
        for (block_root, entry) in entries {
            for index in column_indices(entry.columns) {
                let id = ColumnId::new(*block_root, index)
                    .expect("bitmap bit position is always < NUMBER_OF_COLUMNS");
                match fs::remove_file(self.column_path(&id, entry.slot)) {
                    Ok(()) => removed += 1,
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                }

                self.clear_column(block_root, index);
            }
        }
        Ok(removed)
    }

    /// Clear `column_index`'s bit for `block_root`; once no columns remain,
    /// drop the whole entry.
    fn clear_column(&self, block_root: &B256, column_index: u64) {
        let mut index = self.index_write();
        let now_empty = match index.get_mut(block_root) {
            Some(entry) => {
                entry.columns &= !column_bit(column_index);
                entry.columns == 0
            }
            None => return,
        };
        if now_empty {
            index.remove(block_root);
        }
    }

    /// Recovers from a poisoned lock instead of panicking; the metadata-only
    /// index stays usable.
    fn index_read(&self) -> RwLockReadGuard<'_, HashMap<B256, BlockEntry>> {
        self.index
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn index_write(&self) -> RwLockWriteGuard<'_, HashMap<B256, BlockEntry>> {
        self.index
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl ColumnReadStore for FileColumnStore {
    /// `Ok(None)` is "not present" (including an index entry whose backing
    /// file has vanished); `Err` is an actual storage failure.
    fn get(&self, id: &ColumnId) -> Result<Option<VerifiedColumn>, ColumnStoreError> {
        // An out-of-range index must not be shifted into the bitmap.
        if id.index() >= NUMBER_OF_COLUMNS {
            return Ok(None);
        }

        let Some(entry) = self.index_read().get(&id.block_root()).copied() else {
            return Ok(None);
        };
        if entry.columns & column_bit(id.index()) == 0 {
            return Ok(None);
        }

        let path = self.column_path(id, entry.slot);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                warn!(
                    "index references a column whose file is missing: {}",
                    path.display()
                );
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };

        // Everything in the store was verified before it was written.
        Ok(Some(VerifiedColumn::new_unchecked(
            *id,
            ColumnContext { slot: entry.slot },
            bytes,
        )))
    }

    fn availability(&self, block_root: B256) -> Result<ColumnAvailability, ColumnStoreError> {
        let held = self
            .index_read()
            .get(&block_root)
            .map(|e| e.columns)
            .unwrap_or(0);
        // Full-custody MVP: every column is expected. Custody groups would
        // pass the node's actual custody set here instead.
        Ok(ColumnAvailability::new(held, ALL_COLUMNS_MASK))
    }

    fn get_retention_floor(&self) -> u64 {
        // Relaxed is enough: the floor is a monotonic scalar and carries no
        // ordering relationship with other memory; durability comes from the
        // persisted file, not the atomic.
        self.retention_floor.load(Ordering::Relaxed)
    }

    fn is_below_retention(&self, slot: u64) -> bool {
        slot < self.get_retention_floor()
    }
}

impl ColumnWriteStore for FileColumnStore {
    /// Idempotent per id: a duplicate put keeps the stored column. New columns
    /// are written temp-file + `sync_all` + `rename`, so readers never see a
    /// half-written file.
    fn put(&self, column: VerifiedColumn) -> Result<InsertOutcome, ColumnStoreError> {
        let id = column.id();
        let slot = column.context().slot;
        let block_root = id.block_root();
        let column_index = id.index();

        if self.is_below_retention(slot) {
            return Ok(InsertOutcome::BelowRetention);
        }

        let already_stored = self
            .index_read()
            .get(&block_root)
            .is_some_and(|entry| entry.columns & column_bit(column_index) != 0);
        if already_stored {
            trace!("ignoring duplicate column index={column_index} block_root={block_root:x}");
            return Ok(InsertOutcome::Duplicated);
        }

        fs::create_dir_all(&self.root)?;
        let path = self.column_path(&id, slot);
        let tmp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(column.payload())?;
        file.sync_all()?;
        fs::rename(&tmp_path, &path)?;

        self.index_write()
            .entry(block_root)
            .or_insert(BlockEntry { slot, columns: 0 })
            .columns |= column_bit(column_index);

        debug!("stored column index={column_index} slot={slot} block_root={block_root:x}");
        Ok(InsertOutcome::Inserted)
    }

    fn prune_below_slot(&self, slot: u64) -> Result<usize, ColumnStoreError> {
        if self.is_below_retention(slot) {
            info!(
                "ignoring retention hint below the current floor: hint slot {slot}, floor {floor}",
                floor = self.get_retention_floor()
            );
            return Ok(0);
        }

        self.persist_retention_floor(slot)?;
        self.retention_floor.fetch_max(slot, Ordering::Relaxed);

        let entries = self
            .index_read()
            .iter()
            .filter(|(_, entry)| entry.slot < slot)
            .map(|(root, entry)| (*root, *entry))
            .collect::<Vec<(B256, BlockEntry)>>();
        self.remove_entries(&entries)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use alloy_primitives::B256;
    use ream_data_availability::{
        column::{ColumnContext, VerifiedColumn},
        id::ColumnId,
        store::{ColumnReadStore, ColumnWriteStore, InsertOutcome},
    };

    use super::{BlockEntry, FileColumnStore, RETENTION_FLOOR_FILE};

    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("ream-data-store-test-{pid}-{n}"))
    }

    fn sample_column(block_root: B256, index: u64, slot: u64, payload: &[u8]) -> VerifiedColumn {
        let id = ColumnId::new(block_root, index).expect("index within range");
        VerifiedColumn::new_unchecked(id, ColumnContext { slot }, payload.to_vec())
    }

    #[test]
    fn put_writes_file_and_records_index() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let column = sample_column(B256::repeat_byte(1), 3, 42, b"payload-bytes");
        let id = column.id();

        let outcome = store.put(column).expect("put succeeds");

        assert_eq!(outcome, InsertOutcome::Inserted);
        assert_eq!(
            store.index_read().get(&id.block_root()).copied(),
            Some(BlockEntry {
                slot: 42,
                columns: 1u128 << id.index(),
            }),
        );
        let on_disk = fs::read(store.column_path(&id, 42)).expect("column file written");
        assert_eq!(on_disk.as_slice(), b"payload-bytes");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn put_records_multiple_columns_of_a_block_in_one_entry() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let block_root = B256::repeat_byte(4);

        store
            .put(sample_column(block_root, 2, 30, b"col-2"))
            .expect("put column 2");
        store
            .put(sample_column(block_root, 5, 30, b"col-5"))
            .expect("put column 5");

        assert_eq!(
            store.index_read().get(&block_root).copied(),
            Some(BlockEntry {
                slot: 30,
                columns: (1u128 << 2) | (1u128 << 5),
            }),
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn put_existing_id_is_duplicate_and_keeps_original() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let block_root = B256::repeat_byte(3);
        let id = ColumnId::new(block_root, 0).expect("index within range");

        let first = store
            .put(sample_column(block_root, 0, 10, b"original"))
            .expect("first put");
        // Same id with different bytes, then with a different slot: both are
        // idempotent duplicates.
        let second = store
            .put(sample_column(block_root, 0, 10, b"tampered"))
            .expect("second put");
        let third = store
            .put(sample_column(block_root, 0, 11, b"original"))
            .expect("third put");

        assert_eq!(first, InsertOutcome::Inserted);
        assert_eq!(second, InsertOutcome::Duplicated);
        assert_eq!(third, InsertOutcome::Duplicated);

        let fetched = store.get(&id).expect("get succeeds").expect("present");
        assert_eq!(fetched.payload(), b"original");
        assert_eq!(fetched.context().slot, 10);
        // The ignored slot left no orphan file behind.
        assert!(!store.column_path(&id, 11).exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn get_returns_the_stored_column() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let column = sample_column(B256::repeat_byte(9), 5, 77, b"column-bytes");
        let id = column.id();

        store.put(column.clone()).expect("put succeeds");
        let fetched = store.get(&id).expect("get succeeds");

        assert_eq!(fetched, Some(column));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn get_unknown_id_returns_none() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let id = ColumnId::new(B256::repeat_byte(2), 1).expect("index within range");

        assert_eq!(store.get(&id).expect("get succeeds"), None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn get_returns_none_when_backing_file_is_missing() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let column = sample_column(B256::repeat_byte(6), 4, 20, b"bytes");
        let id = column.id();

        store.put(column).expect("put succeeds");
        // The index still references the removed file.
        fs::remove_file(store.column_path(&id, 20)).expect("remove backing file");

        assert_eq!(store.get(&id).expect("get succeeds"), None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn new_removes_leftover_temp_files() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        let tmp = root.join("deadbeef_0_0.tmp");
        fs::write(&tmp, b"half written").expect("write temp file");

        let store = FileColumnStore::new(root.clone()).expect("open succeeds");

        assert!(!tmp.exists(), "leftover temp file should be cleaned up");
        assert!(store.index_read().is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn retention_floor_survives_reopen() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        // A fresh store starts with no floor.
        assert_eq!(store.retention_floor.load(Ordering::Relaxed), 0);

        store.persist_retention_floor(42).expect("persist floor");
        drop(store);

        let reopened = FileColumnStore::new(root.clone()).expect("reopen store");
        assert_eq!(reopened.retention_floor.load(Ordering::Relaxed), 42);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn malformed_retention_floor_file_degrades_to_no_floor() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join(RETENTION_FLOOR_FILE), b"not-a-slot").expect("write junk");

        // Fail-open: a corrupt floor file must not brick the store.
        let store = FileColumnStore::new(root.clone()).expect("open store");
        assert_eq!(store.retention_floor.load(Ordering::Relaxed), 0);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_removes_old_keeps_recent() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");

        let old = B256::repeat_byte(1);
        let recent = B256::repeat_byte(2);
        store.put(sample_column(old, 0, 10, b"old-0")).expect("put");
        store.put(sample_column(old, 4, 10, b"old-4")).expect("put");
        store
            .put(sample_column(recent, 1, 20, b"recent-1"))
            .expect("put");

        let removed = store.prune_below_slot(15).expect("prune");
        assert_eq!(removed, 2, "both old column files are removed");

        assert!(store.index_read().get(&old).is_none());
        let old_0 = ColumnId::new(old, 0).expect("valid index");
        let old_4 = ColumnId::new(old, 4).expect("valid index");
        assert!(!store.column_path(&old_0, 10).exists());
        assert!(!store.column_path(&old_4, 10).exists());
        assert_eq!(store.get(&old_0).expect("get"), None);

        let recent_1 = ColumnId::new(recent, 1).expect("valid index");
        assert!(store.index_read().get(&recent).is_some());
        assert!(store.column_path(&recent_1, 20).exists());
        assert!(store.get(&recent_1).expect("get").is_some());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_keeps_block_exactly_at_cutoff() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let block = B256::repeat_byte(7);
        store
            .put(sample_column(block, 0, 32, b"at-cutoff"))
            .expect("put");

        // slot 32 is not < 32, so a cutoff equal to the slot keeps the block.
        assert_eq!(store.prune_below_slot(32).expect("prune"), 0);
        assert!(store.index_read().get(&block).is_some());

        assert_eq!(store.prune_below_slot(33).expect("prune"), 1);
        assert!(store.index_read().get(&block).is_none());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_leaves_no_stale_index_or_availability() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let block = B256::repeat_byte(9);
        for index in [0u64, 5, 7] {
            store
                .put(sample_column(block, index, 8, b"x"))
                .expect("put");
        }
        assert_eq!(
            store
                .availability(block)
                .expect("availability")
                .held_count(),
            3
        );

        store.prune_below_slot(100).expect("prune");

        // The bitmap reached 0, so the whole entry is removed.
        assert!(store.index_read().get(&block).is_none());
        assert_eq!(
            store
                .availability(block)
                .expect("availability")
                .held_count(),
            0
        );
        for index in [0u64, 5, 7] {
            let id = ColumnId::new(block, index).expect("valid index");
            assert_eq!(store.get(&id).expect("get"), None);
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_tolerates_an_already_missing_file() {
        let root = temp_root();
        let store = FileColumnStore::new(root.clone()).expect("open store");
        let block = B256::repeat_byte(3);
        store.put(sample_column(block, 0, 5, b"a")).expect("put");
        store.put(sample_column(block, 2, 5, b"b")).expect("put");

        let gone = ColumnId::new(block, 0).expect("valid index");
        fs::remove_file(store.column_path(&gone, 5)).expect("remove file");

        // NotFound is tolerated; only the file still present counts.
        assert_eq!(store.prune_below_slot(10).expect("prune"), 1);
        assert!(store.index_read().get(&block).is_none());

        fs::remove_dir_all(&root).ok();
    }
}
