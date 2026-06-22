use serde::{Deserialize, Serialize};

use crate::id::DaColumnId;

/// Opaque, scheme-specific encoding of a DA column payload together with its
/// availability evidence.
///
/// The DA core never interprets these bytes. For the PeerDAS backend they are
/// an SSZ-encoded `DataColumnSidecar` (cells, KZG commitments, KZG proofs,
/// signed block header, and commitments inclusion proof). A future non-KZG
/// backend can encode different evidence without changing storage, API, or
/// serving logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaPayload(Vec<u8>);

impl DaPayload {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

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
    pub payload: DaPayload,
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
    payload: DaPayload,
}

impl VerifiedColumn {
    /// Construct a verified column without running verification.
    ///
    /// Must only be called by `DaVerifier` implementations (and tests). Any
    /// other call site breaks the invariant that stored data is verified.
    pub fn new_unchecked(id: DaColumnId, context: DaContext, payload: DaPayload) -> Self {
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

    pub fn payload(&self) -> &DaPayload {
        &self.payload
    }

    pub fn into_payload(self) -> DaPayload {
        self.payload
    }
}
