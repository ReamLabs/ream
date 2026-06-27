use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::PathBuf,
    str::FromStr,
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use alloy_primitives::B256;
use ream_da::{
    availability::DaAvailability,
    column::{DaContext, DaPayload, VerifiedColumn},
    error::DaStoreError,
    id::{ALL_COLUMNS_MASK, DaColumnId, NUMBER_OF_COLUMNS, column_indices},
    store::{DaReadStore, DaWriteStore, InsertOutcome},
};
use tracing::{debug, info, trace, warn};

/// Extension of a committed column file. Temp files use `.tmp` instead, so the
/// two are easy to tell apart while scanning the directory.
const COLUMN_FILE_EXTENSION: &str = "ssz";

/// Per-block index entry: the block's slot, plus a 128-bit bitmap marking which
/// column indices are stored for it (bit `i` set ⇔ column `i` present).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlockEntry {
    slot: u64,
    columns: u128,
}

/// Bit mask selecting `column_index` within a block's 128-column presence
/// bitmap. `column_index` is always `< NUMBER_OF_COLUMNS` for a valid id, so the
/// shift never exceeds the width of `u128`.
fn column_bit(column_index: u64) -> u128 {
    debug_assert!(column_index < NUMBER_OF_COLUMNS);
    1u128 << column_index
}

/// File-backed DA store.
///
/// Each verified column is persisted as its own file under `root`, so the
/// filesystem is the source of truth for the column bytes — there is no
/// in-memory copy of the payloads. A small in-memory index maps each block root
/// to its slot and the set of columns stored for it, so reads can locate the
/// backing file and check presence from an id alone; it holds only metadata and
/// is rebuilt from the directory by [`DaFileStore::new`].
pub struct DaFileStore {
    /// Root directory holding one file per stored column, typically derived
    /// from the CLI `--data-dir`. Created lazily on first write.
    root: PathBuf,

    /// In-memory index from block root to its [`BlockEntry`] (slot + column
    /// bitmap). Lets `get` recover the slot — and therefore the file name — and
    /// check column presence from an id alone, without touching the filesystem.
    /// Rebuilt by scanning the directory in [`DaFileStore::new`].
    ///
    /// Sized per block: over the ~18-day retention window
    /// (4096 epochs × 32 slots) that is at most ~131k entries
    index: RwLock<HashMap<B256, BlockEntry>>,
    // TODO add a payload cache, to avoid reading from files everytime.
    //      problem: it'll occupy a lot of memory size, we should keep cache short and up-to-date
}

impl DaFileStore {
    /// Open a store rooted at `root`, rebuilding the in-memory index from the
    /// column files already on disk.
    ///
    /// This is the constructor to use on node startup: columns written by an
    /// earlier run become available again. A missing `root` is not an error —
    /// it yields an empty store that creates the directory on its first write.
    /// Leftover `*.tmp` files from an interrupted write are removed.
    pub fn new(da_root: PathBuf) -> Result<Self, DaStoreError> {
        let store = Self {
            root: da_root,
            index: RwLock::new(HashMap::new()),
        };
        store.rebuild_index()?;
        Ok(store)
    }

    /// Path of the file backing `(id, slot)`.
    ///
    /// The name is `{slot:08}_{index:03}_{block_root:x}.ssz`
    /// rebuild the index from a directory scan.
    fn column_path(&self, id: &DaColumnId, slot: u64) -> PathBuf {
        let block_root = id.block_root();
        let index = id.index();
        self.root.join(format!(
            "{slot:08}_{index:03}_{block_root:x}.{COLUMN_FILE_EXTENSION}"
        ))
    }

    /// Scan `root` and populate the index from the column files present,
    /// deleting any stray temp files left behind by an interrupted write.
    fn rebuild_index(&self) -> Result<(), DaStoreError> {
        let root = self.root.display();
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            // No directory yet means nothing has been stored, which is normal.
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                debug!("no DA store directory at {root} yet; starting with an empty index");
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

            // A half-written file from a crashed `put` is never trustworthy, remove it.
            if path.extension().and_then(|ext| ext.to_str()) == Some("tmp") {
                debug!(
                    "removing leftover temp file from an interrupted write: {}",
                    path.display()
                );
                let _ = fs::remove_file(&path);
                continue;
            }

            // Skip anything we did not write (unexpected names, stray files):
            // startup must not fail on directory clutter.
            if let Some((id, slot)) = Self::parse_column_file_name(name) {
                index
                    .entry(id.block_root())
                    .or_insert(BlockEntry { slot, columns: 0 })
                    .columns |= column_bit(id.index());
                column_files += 1;
            }
        }

        info!(
            "loaded DA store index from {root}: {column_files} column files across {} blocks",
            index.len()
        );
        Ok(())
    }

    /// Parse a `{slot:08}_{index:03}_{block_root:x}.ssz` file name back into its
    /// id and slot, the inverse of [`DaFileStore::column_path`]. Returns `None`
    /// for any name that does not match exactly, so unrelated files are simply
    /// skipped.
    fn parse_column_file_name(name: &str) -> Option<(DaColumnId, u64)> {
        let stem = name.strip_suffix(&format!(".{COLUMN_FILE_EXTENSION}"))?;
        let mut parts = stem.split('_');
        let slot = parts.next()?.parse::<u64>().ok()?;
        let index = parts.next()?.parse::<u64>().ok()?;
        let block_root = B256::from_str(parts.next()?).ok()?;
        // Reject names carrying extra `_`-separated fields we never emit.
        if parts.next().is_some() {
            return None;
        }
        let id = DaColumnId::new(block_root, index).ok()?;
        Some((id, slot))
    }

    fn remove_entries(&self, entries: &[(B256, BlockEntry)]) -> Result<usize, DaStoreError> {
        let mut removed = 0;
        for (block_root, entry) in entries {
            for index in column_indices(entry.columns) {
                let id = DaColumnId::new(*block_root, index)
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

    /// Clear `column_index`'s bit for `block_root`; once no columns remain, drop the
    /// whole entry. Keeps the in-memory bitmap in step with the files on disk.
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

    /// Read guard over the index, recovering from a poisoned lock instead of
    /// panicking (a poisoned in-memory index is still readable).
    fn index_read(&self) -> RwLockReadGuard<'_, HashMap<B256, BlockEntry>> {
        self.index
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Write guard over the index, recovering from a poisoned lock.
    fn index_write(&self) -> RwLockWriteGuard<'_, HashMap<B256, BlockEntry>> {
        self.index
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl DaReadStore for DaFileStore {
    /// Fetch a stored column by id.
    ///
    /// `Ok(None)` means "not present here" — a normal answer for a serving node
    /// — and also covers an index entry whose backing file has since vanished.
    /// `Err` is reserved for actual storage failures (I/O, corruption).
    fn get(&self, id: &DaColumnId) -> Result<Option<VerifiedColumn>, DaStoreError> {
        // An out-of-range column index can never have been stored, and must not
        // be shifted into the bitmap; treat it as absent.
        if id.index() >= NUMBER_OF_COLUMNS {
            return Ok(None);
        }

        // Locate the block and confirm this column's bit is set, all in memory.
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

        // Everything in the store was verified before it was written, so
        // reconstructing it as a `VerifiedColumn` upholds the invariant.
        Ok(Some(VerifiedColumn::new_unchecked(
            *id,
            DaContext { slot: entry.slot },
            DaPayload::new(bytes),
        )))
    }

    fn availability(&self, block_root: B256) -> Result<DaAvailability, DaStoreError> {
        let held = self
            .index_read()
            .get(&block_root)
            .map(|e| e.columns)
            .unwrap_or(0);
        // Full-custody MVP: every column is expected. When custody groups land,
        // this is where the node's actual custody set would be passed instead.
        Ok(DaAvailability::new(held, ALL_COLUMNS_MASK))
    }
}

impl DaWriteStore for DaFileStore {
    /// Store a verified column.
    ///
    /// Storage is keyed by id. Once a column is stored for an id, any later put
    /// for that id is an idempotent [`InsertOutcome::Duplicated`]: the incoming
    /// column is ignored and the stored one is kept. A new column is written
    /// with temp-file + `sync_all` + `rename`, so readers never trust a
    /// half-written file.
    fn put(&self, column: VerifiedColumn) -> Result<InsertOutcome, DaStoreError> {
        let id = column.id();
        let slot = column.context().slot;
        let block_root = id.block_root();
        let column_index = id.index();

        // Already stored (this column's bit is set for its block)? Idempotent.
        let already_stored = self
            .index_read()
            .get(&block_root)
            .is_some_and(|entry| entry.columns & column_bit(column_index) != 0);
        if already_stored {
            trace!("ignoring duplicate column index={column_index} block_root={block_root:x}");
            return Ok(InsertOutcome::Duplicated);
        }

        // New column: write to a temp file, flush it to disk, then atomically
        // rename it into place before recording it in the index.
        fs::create_dir_all(&self.root)?;
        let path = self.column_path(&id, slot);
        let tmp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(column.payload().as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp_path, &path)?;

        self.index_write()
            .entry(block_root)
            .or_insert(BlockEntry { slot, columns: 0 })
            .columns |= column_bit(column_index);

        debug!("stored column index={column_index} slot={slot} block_root={block_root:x}");
        Ok(InsertOutcome::Inserted)
    }

    fn prune_below_slot(&self, slot: u64) -> Result<usize, DaStoreError> {
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
    use ream_da::{
        column::{DaContext, DaPayload, VerifiedColumn},
        id::DaColumnId,
        store::{DaReadStore, DaWriteStore, InsertOutcome},
    };

    use super::{BlockEntry, DaFileStore};

    /// A unique temp directory per call, so tests don't collide when run in
    /// parallel. No `tempfile` dependency: the path is process- and
    /// counter-scoped, and the store creates it lazily on first write.
    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("ream-da-store-test-{pid}-{n}"))
    }

    fn sample_column(block_root: B256, index: u64, slot: u64, payload: &[u8]) -> VerifiedColumn {
        let id = DaColumnId::new(block_root, index).expect("index within range");
        VerifiedColumn::new_unchecked(id, DaContext { slot }, DaPayload::new(payload.to_vec()))
    }

    #[test]
    fn put_writes_file_and_records_index() {
        let root = temp_root();
        let store = DaFileStore::new(root.clone()).expect("open store");
        let column = sample_column(B256::repeat_byte(1), 3, 42, b"payload-bytes");
        let id = column.id();

        let outcome = store.put(column).expect("put succeeds");

        assert_eq!(outcome, InsertOutcome::Inserted);
        // One per-block entry: slot 42, with only column 3 marked present.
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
        let store = DaFileStore::new(root.clone()).expect("open store");
        let block_root = B256::repeat_byte(4);

        store
            .put(sample_column(block_root, 2, 30, b"col-2"))
            .expect("put column 2");
        store
            .put(sample_column(block_root, 5, 30, b"col-5"))
            .expect("put column 5");

        // Both columns share a single per-block entry, with both bits set.
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
        let store = DaFileStore::new(root.clone()).expect("open store");
        let block_root = B256::repeat_byte(3);
        let id = DaColumnId::new(block_root, 0).expect("index within range");

        let first = store
            .put(sample_column(block_root, 0, 10, b"original"))
            .expect("first put");
        // Same id, different bytes: ignored, treated as an idempotent duplicate.
        let second = store
            .put(sample_column(block_root, 0, 10, b"tampered"))
            .expect("second put");
        // Same id, different slot: also a duplicate; no second file is written.
        let third = store
            .put(sample_column(block_root, 0, 11, b"original"))
            .expect("third put");

        assert_eq!(first, InsertOutcome::Inserted);
        assert_eq!(second, InsertOutcome::Duplicated);
        assert_eq!(third, InsertOutcome::Duplicated);

        // The originally stored column is untouched...
        let fetched = store.get(&id).expect("get succeeds").expect("present");
        assert_eq!(fetched.payload().as_bytes(), b"original");
        assert_eq!(fetched.context().slot, 10);
        // ...and the ignored slot left no orphan file behind.
        assert!(!store.column_path(&id, 11).exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn get_returns_the_stored_column() {
        let root = temp_root();
        let store = DaFileStore::new(root.clone()).expect("open store");
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
        let store = DaFileStore::new(root.clone()).expect("open store");
        let id = DaColumnId::new(B256::repeat_byte(2), 1).expect("index within range");

        assert_eq!(store.get(&id).expect("get succeeds"), None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn get_returns_none_when_backing_file_is_missing() {
        let root = temp_root();
        let store = DaFileStore::new(root.clone()).expect("open store");
        let column = sample_column(B256::repeat_byte(6), 4, 20, b"bytes");
        let id = column.id();

        store.put(column).expect("put succeeds");
        // Remove the file out-of-band while the index still references it.
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

        let store = DaFileStore::new(root.clone()).expect("open succeeds");

        assert!(!tmp.exists(), "leftover temp file should be cleaned up");
        assert!(store.index_read().is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_removes_old_keeps_recent() {
        let root = temp_root();
        let store = DaFileStore::new(root.clone()).expect("open store");

        // Old block at slot 10 (two columns); recent block at slot 20 (one).
        let old = B256::repeat_byte(1);
        let recent = B256::repeat_byte(2);
        store.put(sample_column(old, 0, 10, b"old-0")).expect("put");
        store.put(sample_column(old, 4, 10, b"old-4")).expect("put");
        store
            .put(sample_column(recent, 1, 20, b"recent-1"))
            .expect("put");

        // Cutoff 15: slot 10 < 15 is pruned, slot 20 is kept.
        let removed = store.prune_below_slot(15).expect("prune");
        assert_eq!(removed, 2, "both old column files are removed");

        // Old block: index entry gone, files gone, get -> None.
        assert!(store.index_read().get(&old).is_none());
        let old_0 = DaColumnId::new(old, 0).expect("valid index");
        let old_4 = DaColumnId::new(old, 4).expect("valid index");
        assert!(!store.column_path(&old_0, 10).exists());
        assert!(!store.column_path(&old_4, 10).exists());
        assert_eq!(store.get(&old_0).expect("get"), None);

        // Recent block: untouched (index + file + get).
        let recent_1 = DaColumnId::new(recent, 1).expect("valid index");
        assert!(store.index_read().get(&recent).is_some());
        assert!(store.column_path(&recent_1, 20).exists());
        assert!(store.get(&recent_1).expect("get").is_some());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_keeps_block_exactly_at_cutoff() {
        let root = temp_root();
        let store = DaFileStore::new(root.clone()).expect("open store");
        let block = B256::repeat_byte(7);
        store
            .put(sample_column(block, 0, 32, b"at-cutoff"))
            .expect("put");

        // slot 32 is not < 32, so a cutoff equal to the slot keeps the block.
        assert_eq!(store.prune_below_slot(32).expect("prune"), 0);
        assert!(store.index_read().get(&block).is_some());

        // Raising the cutoff one past the slot prunes it.
        assert_eq!(store.prune_below_slot(33).expect("prune"), 1);
        assert!(store.index_read().get(&block).is_none());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_leaves_no_stale_index_or_availability() {
        let root = temp_root();
        let store = DaFileStore::new(root.clone()).expect("open store");
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

        // The whole entry is gone (bitmap reached 0 -> entry removed), so the
        // bitmap never lingers claiming columns whose files were deleted.
        assert!(store.index_read().get(&block).is_none());
        assert_eq!(
            store
                .availability(block)
                .expect("availability")
                .held_count(),
            0
        );
        for index in [0u64, 5, 7] {
            let id = DaColumnId::new(block, index).expect("valid index");
            assert_eq!(store.get(&id).expect("get"), None);
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prune_below_slot_tolerates_an_already_missing_file() {
        let root = temp_root();
        let store = DaFileStore::new(root.clone()).expect("open store");
        let block = B256::repeat_byte(3);
        store.put(sample_column(block, 0, 5, b"a")).expect("put");
        store.put(sample_column(block, 2, 5, b"b")).expect("put");

        // Delete one backing file out of band; the index still references it.
        let gone = DaColumnId::new(block, 0).expect("valid index");
        fs::remove_file(store.column_path(&gone, 5)).expect("remove file");

        // Prune still succeeds (NotFound tolerated) and clears the whole block;
        // only the file that was still present counts toward the total.
        assert_eq!(store.prune_below_slot(10).expect("prune"), 1);
        assert!(store.index_read().get(&block).is_none());

        fs::remove_dir_all(&root).ok();
    }
}
