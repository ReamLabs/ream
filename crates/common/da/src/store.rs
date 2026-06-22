use crate::{column::VerifiedColumn, error::DaStoreError, id::DaColumnId};

/// Outcome of inserting a verified column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The column was newly persisted.
    Inserted,
    /// The same identifier already held identical content; the insert is a
    /// no-op (idempotent success).
    Duplicated,
}

/// Read-only storage handle.
///
/// This is the only storage capability handed to the local API and to P2P
/// serving. Serving does not re-verify on the output path because the store
/// only ever contains verified data.
pub trait DaReadStore: Send + Sync {
    fn get(&self, id: &DaColumnId) -> Result<Option<VerifiedColumn>, DaStoreError>;
}

/// Write-capable storage handle.
///
/// Handed to the verification service only. Accepting [`VerifiedColumn`] (not
/// candidates) makes "unverified data is never stored" a type-level rule.
pub trait DaWriteStore: DaReadStore {
    /// Store a verified column.
    ///
    /// Inserting identical content twice is not allowed. Inserting different
    /// content under an existing identifier returns
    /// [`DaStoreError::DuplicateConflict`] and keeps the existing value:
    /// storage never silently overwrites a verified column.
    fn put(&self, column: VerifiedColumn) -> Result<InsertOutcome, DaStoreError>;
}
