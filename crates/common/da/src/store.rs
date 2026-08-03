use alloy_primitives::B256;

use crate::{
    availability::DaAvailability, column::VerifiedColumn, error::DaStoreError, id::DaColumnId,
};

/// Outcome of inserting a verified column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    /// A column was already stored for this id; the insert is an idempotent
    /// no-op and the existing column is kept.
    Duplicated,
}

/// Read-only storage handle. Serving never re-verifies on the output path
/// because the store only ever contains verified data.
pub trait DaReadStore: Send + Sync {
    fn get(&self, id: &DaColumnId) -> Result<Option<VerifiedColumn>, DaStoreError>;
    fn availability(&self, block_root: B256) -> Result<DaAvailability, DaStoreError>;
}

/// Write-capable storage handle, handed to the verification service only.
/// Accepting [`VerifiedColumn`] (not candidates) makes "unverified data is
/// never stored" a type-level rule.
pub trait DaWriteStore: DaReadStore {
    fn put(&self, column: VerifiedColumn) -> Result<InsertOutcome, DaStoreError>;

    fn prune_below_slot(&self, slot: u64) -> Result<usize, DaStoreError>;
}
