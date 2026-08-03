use serde::{Deserialize, Serialize};

use crate::id::DaColumnId;

/// Consensus-derived context attached to a candidate column.
///
/// Only plain data crosses this boundary; no beacon runtime handles.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DaContext {
    /// Slot of the block the column belongs to. Used for retention decisions,
    /// never for fork choice.
    pub slot: u64,
}

/// A candidate column submitted for verification.
///
/// Candidates may come from a consensus data source, the dev-mode ingest API,
/// or (in the future) peers. All of them pass through the same verification
/// pipeline before they can be stored or served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateColumn {
    pub id: DaColumnId,
    pub context: DaContext,
    /// Opaque, scheme-specific payload bytes carrying the column and its
    /// availability evidence. The DA core never interprets them: for the
    /// PeerDAS backend they are an SSZ-encoded `DataColumnSidecar` (cells, KZG
    /// commitments, KZG proofs, signed block header, and commitments inclusion
    /// proof); a future non-KZG backend can encode different evidence without
    /// changing storage, API, or serving logic.
    pub payload: Vec<u8>,
}

/// A column that passed verification.
///
/// This is the only type accepted by `DaWriteStore`. It must only be
/// constructed by `DaVerifier` implementations; everything downstream of the
/// verifier relies on this to avoid re-verifying on the serving path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedColumn {
    id: DaColumnId,
    context: DaContext,
    payload: Vec<u8>,
}

impl VerifiedColumn {
    /// Construct a verified column without running verification.
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

    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }
}
