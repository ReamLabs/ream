use ream_da::column::CandidateColumn;
use tokio::sync::mpsc;

use crate::error::IngestionError;

/// Work delivered to the verification service over the ingest channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaWorkItem {
    /// A candidate column to verify and, if valid, store.
    Candidate(CandidateColumn),
    /// A beacon-issued retention boundary: prune stored columns older than it.
    /// It rides the same queue as candidates so the single consumer applies it
    /// in order, preserving the store's single-writer assumption.
    Retention(RetentionHint),
}

/// A retention boundary issued by the (trusted) beacon.
///
/// The beacon owns the retention policy and computes the boundary; the DA node
/// just obeys. See [`crate::service::DaVerificationService`] for how it is
/// applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionHint {
    /// Prune every stored column whose slot is strictly below this.
    pub slot: u64,
}

/// Cloneable submission handle for the verification queue.
///
/// Every candidate source holds a clone of this handle and submits through it,
/// so all candidates funnel into the same queue and the same verifier. The
/// handle only moves work onto the queue; it performs no verification.
#[derive(Clone)]
pub struct DaIngestHandle {
    sender: mpsc::Sender<DaWorkItem>,
}

impl DaIngestHandle {
    /// Submit a candidate, awaiting while the queue is full.
    ///
    /// Applies backpressure: a fast producer slows to the verifier's pace rather
    /// than dropping work. Fails only with [`IngestionError::Closed`] once the
    /// verification service has stopped.
    pub async fn submit(&self, candidate: CandidateColumn) -> Result<(), IngestionError> {
        self.sender
            .send(DaWorkItem::Candidate(candidate))
            .await
            .map_err(|_| IngestionError::Closed)
    }

    /// Submit a candidate without waiting.
    ///
    /// Returns [`IngestionError::Overloaded`] immediately when the queue is full,
    /// so callers that must not block (e.g. an RPC handler answering with 503)
    /// can shed load instead of buffering unbounded. Returns
    /// [`IngestionError::Closed`] once the verification service has stopped.
    pub fn try_submit(&self, candidate: CandidateColumn) -> Result<(), IngestionError> {
        self.sender
            .try_send(DaWorkItem::Candidate(candidate))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => IngestionError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => IngestionError::Closed,
            })
    }

    /// Submit a retention hint, awaiting while the queue is full.
    ///
    /// Travels the same queue as candidates, so the single consumer serializes
    /// pruning with verification. Fails only with [`IngestionError::Closed`]
    /// once the verification service has stopped.
    pub async fn submit_retention(&self, hint: RetentionHint) -> Result<(), IngestionError> {
        self.sender
            .send(DaWorkItem::Retention(hint))
            .await
            .map_err(|_| IngestionError::Closed)
    }

    /// Submit a retention hint without waiting.
    ///
    /// Like [`Self::try_submit`], returns [`IngestionError::Overloaded`] when the
    /// queue is full so a non-blocking caller (e.g. an RPC handler) can shed load
    /// instead of buffering.
    pub fn try_submit_retention(&self, hint: RetentionHint) -> Result<(), IngestionError> {
        self.sender
            .try_send(DaWorkItem::Retention(hint))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => IngestionError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => IngestionError::Closed,
            })
    }
}

/// Create the bounded ingest queue.
///
/// Returns the producer-side [`DaIngestHandle`] (clone it for each candidate
/// source) and the consumer-side receiver, which is handed to the single
/// verification service.
pub fn ingest_channel(capacity: usize) -> (DaIngestHandle, mpsc::Receiver<DaWorkItem>) {
    let (sender, receiver) = mpsc::channel(capacity);
    (DaIngestHandle { sender }, receiver)
}
