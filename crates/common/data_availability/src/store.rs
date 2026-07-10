use alloy_primitives::B256;

use crate::{
    availability::ColumnAvailability, column::VerifiedColumn, error::ColumnStoreError, id::ColumnId,
};

/// Outcome of inserting a verified column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    /// A column was already stored for this id; the insert is an idempotent
    /// no-op and the existing column is kept.
    Duplicated,
    /// The column's slot is below the retention floor: it was refused and
    /// nothing was stored.
    BelowRetention,
}

/// Read-only storage handle. Serving never re-verifies on the output path
/// because the store only ever contains verified data.
pub trait ColumnReadStore: Send + Sync {
    fn get(&self, id: &ColumnId) -> Result<Option<VerifiedColumn>, ColumnStoreError>;
    fn availability(&self, block_root: B256) -> Result<ColumnAvailability, ColumnStoreError>;

    /// The current retention floor, as a slot; `0` means no floor yet.
    fn get_retention_floor(&self) -> u64;

    /// Whether a column at `slot` is strictly older than the retention floor
    /// — the exact predicate [`DaWriteStore::put`] refuses on. A column
    /// exactly at the floor is kept.
    fn is_below_retention(&self, slot: u64) -> bool;
}

/// Write-capable storage handle, handed to the verification service only.
/// Accepting [`VerifiedColumn`] (not candidates) makes "unverified data is
/// never stored" a type-level rule.
pub trait ColumnWriteStore: ColumnReadStore {
    fn put(&self, column: VerifiedColumn) -> Result<InsertOutcome, ColumnStoreError>;

    /// Raise the retention floor to `slot` and prune every stored column below
    /// it, returning how many were removed.
    ///
    /// The floor is monotonic and durable: a hint below the current floor is a
    /// no-op, and it must be recorded before any data is deleted so that an
    /// interrupted prune resumes on the next startup.
    fn prune_below_slot(&self, slot: u64) -> Result<usize, ColumnStoreError>;
}
