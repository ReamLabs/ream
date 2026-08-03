use ream_da::column::CandidateColumn;
use tokio::sync::mpsc;

use crate::error::IngestionError;

/// Work delivered to the verification service over the ingest channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaWorkItem {
    Candidate(CandidateColumn),
    /// A beacon-issued retention boundary.
    Retention(RetentionHint),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionHint {
    /// Prune every stored column whose slot is strictly below this.
    pub slot: u64,
}

/// Cloneable submission handle for the verification queue.
#[derive(Clone)]
pub struct DaIngestHandle {
    sender: mpsc::Sender<DaWorkItem>,
}

impl DaIngestHandle {
    /// Submit a candidate, awaiting while the queue is full (backpressure).
    pub async fn submit(&self, candidate: CandidateColumn) -> Result<(), IngestionError> {
        self.sender
            .send(DaWorkItem::Candidate(candidate))
            .await
            .map_err(|_| IngestionError::Closed)
    }

    /// Submit a candidate without waiting; a full queue is
    /// [`IngestionError::Overloaded`] so the caller can shed load.
    pub fn try_submit(&self, candidate: CandidateColumn) -> Result<(), IngestionError> {
        self.sender
            .try_send(DaWorkItem::Candidate(candidate))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => IngestionError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => IngestionError::Closed,
            })
    }

    /// Submit a retention hint, awaiting while the queue is full.
    pub async fn submit_retention(&self, hint: RetentionHint) -> Result<(), IngestionError> {
        self.sender
            .send(DaWorkItem::Retention(hint))
            .await
            .map_err(|_| IngestionError::Closed)
    }

    /// Submit a retention hint without waiting; a full queue is
    /// [`IngestionError::Overloaded`].
    pub fn try_submit_retention(&self, hint: RetentionHint) -> Result<(), IngestionError> {
        self.sender
            .try_send(DaWorkItem::Retention(hint))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => IngestionError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => IngestionError::Closed,
            })
    }
}

/// Create the bounded ingest queue: a cloneable producer handle and the
/// receiver for the single verification service.
pub fn ingest_channel(capacity: usize) -> (DaIngestHandle, mpsc::Receiver<DaWorkItem>) {
    let (sender, receiver) = mpsc::channel(capacity);
    (DaIngestHandle { sender }, receiver)
}
