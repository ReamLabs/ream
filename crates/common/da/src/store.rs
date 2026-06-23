use crate::{column::VerifiedColumn, error::DaStoreError, id::DaColumnId};

/// Outcome of inserting a verified column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The column was newly persisted.
    Inserted,
    /// A column was already stored for this identifier; the insert is a no-op
    /// idempotent success and the existing value is kept.
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
    /// Columns are keyed by [`DaColumnId`]. If a column is already stored for
    /// this id, the call is an idempotent [`InsertOutcome::Duplicated`]: the
    /// incoming column is ignored and the stored one is kept, never
    /// overwritten. Otherwise the column is persisted and
    /// [`InsertOutcome::Inserted`] is returned.
    fn put(&self, column: VerifiedColumn) -> Result<InsertOutcome, DaStoreError>;
}
