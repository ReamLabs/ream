use ream_data_availability::column::{CandidateBlock, CandidateColumn};
use tokio::sync::mpsc;
use tracing::debug;

use crate::error::IngestionError;

/// Work delivered to the verification service over the ingest channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestWorkItem {
    Candidate(CandidateColumn),
    /// A whole block's worth of candidate columns. One block occupies one
    /// queue slot, so admission is all-or-nothing, never split.
    CandidateBlock(CandidateBlock),
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
pub struct IngestHandle {
    sender: mpsc::Sender<IngestWorkItem>,
}

impl IngestHandle {
    /// Submit a candidate, awaiting while the queue is full (backpressure).
    pub async fn submit(&self, candidate: CandidateColumn) -> Result<(), IngestionError> {
        self.sender
            .send(IngestWorkItem::Candidate(candidate))
            .await
            .map_err(|err| {
                debug!("candidate submission failed, receiver dropped: {err}");
                IngestionError::Closed
            })
    }

    /// Submit a candidate without waiting; a full queue is
    /// [`IngestionError::Overloaded`] so the caller can shed load.
    pub fn try_submit(&self, candidate: CandidateColumn) -> Result<(), IngestionError> {
        self.sender
            .try_send(IngestWorkItem::Candidate(candidate))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => IngestionError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => IngestionError::Closed,
            })
    }

    /// Submit a whole block's batch, awaiting while the queue is full.
    pub async fn submit_block(&self, candidate: CandidateBlock) -> Result<(), IngestionError> {
        self.sender
            .send(IngestWorkItem::CandidateBlock(candidate))
            .await
            .map_err(|err| {
                debug!("block submission failed, receiver dropped: {err}");
                IngestionError::Closed
            })
    }

    /// Submit a whole block's batch without waiting.
    pub fn try_submit_block(&self, candidate: CandidateBlock) -> Result<(), IngestionError> {
        self.sender
            .try_send(IngestWorkItem::CandidateBlock(candidate))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => IngestionError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => IngestionError::Closed,
            })
    }

    /// Submit a retention hint, awaiting while the queue is full.
    pub async fn submit_retention(&self, hint: RetentionHint) -> Result<(), IngestionError> {
        self.sender
            .send(IngestWorkItem::Retention(hint))
            .await
            .map_err(|err| {
                debug!("retention submission failed, receiver dropped: {err}");
                IngestionError::Closed
            })
    }

    /// Submit a retention hint without waiting; a full queue is
    /// [`IngestionError::Overloaded`].
    pub fn try_submit_retention(&self, hint: RetentionHint) -> Result<(), IngestionError> {
        self.sender
            .try_send(IngestWorkItem::Retention(hint))
            .map_err(|err| match err {
                mpsc::error::TrySendError::Full(_) => IngestionError::Overloaded,
                mpsc::error::TrySendError::Closed(_) => IngestionError::Closed,
            })
    }
}

/// Create the bounded ingest queue: a cloneable producer handle and the
/// receiver for the single verification service.
pub fn ingest_channel(capacity: usize) -> (IngestHandle, mpsc::Receiver<IngestWorkItem>) {
    let (sender, receiver) = mpsc::channel(capacity);
    (IngestHandle { sender }, receiver)
}
