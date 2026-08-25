use serde::{Deserialize, Serialize};

use crate::id::ColumnId;

/// Consensus-derived context attached to a candidate column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ColumnContext {
    /// Slot of the block the column belongs to; used for retention only.
    pub slot: u64,
}

/// A candidate column submitted for verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateColumn {
    pub id: ColumnId,
    pub context: ColumnContext,
    pub payload: Vec<u8>,
}

/// A column that passed verification — the only type accepted by
/// `ColumnWriteStore`, and only constructed by `ColumnVerifier` implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedColumn {
    id: ColumnId,
    context: ColumnContext,
    payload: Vec<u8>,
}

impl VerifiedColumn {
    pub fn new_unchecked(id: ColumnId, context: ColumnContext, payload: Vec<u8>) -> Self {
        Self {
            id,
            context,
            payload,
        }
    }

    pub fn id(&self) -> ColumnId {
        self.id
    }

    pub fn context(&self) -> ColumnContext {
        self.context
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
