use serde::{Deserialize, Serialize};

use crate::id::DaColumnId;

/// Consensus-derived context attached to a candidate column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DaContext {
    /// Slot of the block the column belongs to; used for retention only.
    pub slot: u64,
}

/// A candidate column submitted for verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateColumn {
    pub id: DaColumnId,
    pub context: DaContext,
    pub payload: Vec<u8>,
}

/// A column that passed verification — the only type accepted by
/// `DaWriteStore`, and only constructed by `DaVerifier` implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedColumn {
    id: DaColumnId,
    context: DaContext,
    payload: Vec<u8>,
}

impl VerifiedColumn {
    pub fn new_unchecked(id: DaColumnId, context: DaContext, payload: Vec<u8>) -> Self {
        Self {
            id,
            context,
            payload,
        }
    }

    pub fn id(&self) -> DaColumnId {
        self.id
    }

    pub fn context(&self) -> DaContext {
        self.context
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
