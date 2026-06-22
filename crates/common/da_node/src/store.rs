use std::{collections::HashMap, path::PathBuf, sync::RwLock};

use ream_da::{
    column::VerifiedColumn,
    error::DaStoreError,
    id::DaColumnId,
    store::{DaReadStore, DaWriteStore, InsertOutcome},
};

/// File-backed DA store.
///
/// Each verified column is persisted as its own file under `root`, so the
/// filesystem is the source of truth: reads and writes go straight to disk and
/// there is no in-memory copy of the column set. The store is shared (behind an
/// `Arc`) between the verification writer and the read-only serving path, so
/// every method takes `&self`.
pub struct DaFileStore {
    /// Root directory holding one file per stored column, typically derived
    /// from the CLI `--data-dir`. Created lazily on first write.
    _root: PathBuf,

    _index: RwLock<HashMap<DaColumnId, u64>>,
    // TODO add a cache, to avoid read from files everytime.
}

impl DaFileStore {
    /// Create a store rooted at `root`.
    ///
    /// There is intentionally no `Default`: a file store without a real
    /// directory would silently write to the current path, so the directory
    /// must be supplied explicitly.
    pub fn new(root: PathBuf) -> Self {
        Self {
            _root: root,
            _index: RwLock::new(HashMap::new()),
        }
    }
}
impl DaReadStore for DaFileStore {
    /// Fetch a stored column by id.
    ///
    /// `Ok(None)` means "not present here" — a normal answer for a serving
    /// node. `Err` is reserved for actual storage failures (I/O, corruption).
    fn get(&self, _id: &DaColumnId) -> Result<Option<VerifiedColumn>, DaStoreError> {
        todo!()
    }
}

impl DaWriteStore for DaFileStore {
    /// Persist a verified column, reporting whether it was newly written or an
    /// idempotent duplicate.
    fn put(&self, _column: VerifiedColumn) -> Result<InsertOutcome, DaStoreError> {
        todo!()
    }
}
