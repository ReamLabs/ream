use std::sync::Arc;

use ream_da::{
    column::CandidateColumn,
    store::{DaWriteStore, InsertOutcome},
    verifier::DaVerifier,
};
use ream_executor::ReamExecutor;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Work delivered to the verification service over the ingest channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaWorkItem {
    /// A candidate column to verify and, if valid, store.
    Candidate(CandidateColumn),
}

/// Runs the DA verification pipeline: drain candidate columns from the ingest
/// channel, verify each one, and persist those that pass.
///
/// This is the only component that writes to the store, which keeps "unverified
/// data is never stored". It is a single consumer, so writes reach the store serialized
///  — matching the file store's single-writer assumption.
pub struct DaVerificationService {
    receiver: mpsc::Receiver<DaWorkItem>,
    verifier: Arc<dyn DaVerifier>,
    store: Arc<dyn DaWriteStore>,
    executor: ReamExecutor,
}

impl DaVerificationService {
    pub fn new(
        receiver: mpsc::Receiver<DaWorkItem>,
        verifier: Arc<dyn DaVerifier>,
        store: Arc<dyn DaWriteStore>,
        executor: ReamExecutor,
    ) -> Self {
        Self {
            receiver,
            verifier,
            store,
            executor,
        }
    }
    /// Consume candidate columns until the ingest channel closes.
    ///
    /// A single sequential consumer: each candidate is verified and stored
    /// before the next is taken. That caps throughput but keeps storage writes
    /// ordered and serialized;
    /// TODO: batching can come once a real (batchable) verifier exists.
    pub async fn run(mut self) {
        info!("DA verification service started");
        while let Some(item) = self.receiver.recv().await {
            match item {
                DaWorkItem::Candidate(candidate) => self.process_candidate(candidate).await,
            }
        }
        info!("DA verification service stopped: ingestion queue closed");
    }

    /// Verify a single candidate and, if it passes, persist it.
    async fn process_candidate(&self, candidate: CandidateColumn) {
        let id = candidate.id;
        let verifier = self.verifier.clone();

        // Verify
        let verified = match self
            .executor
            .spawn_blocking(move || verifier.verify(candidate))
            .await
        {
            Ok(result) => result,
            Err(err) => {
                error!("verification worker panicked or was cancelled: {err}");
                return;
            }
        };

        // On success persist the column; a rejected candidate is simply dropped.
        match verified {
            Ok(verified_column) => {
                let store = self.store.clone();
                let outcome = match self
                    .executor
                    .spawn_blocking(move || store.put(verified_column))
                    .await
                {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        error!("storage worker panicked or was cancelled: {err}");
                        return;
                    }
                };

                match outcome {
                    Ok(InsertOutcome::Inserted) => {
                        debug!(
                            "stored verified column: block root {root}, column {index}",
                            root = id.block_root(),
                            index = id.index()
                        );
                    }
                    Ok(InsertOutcome::Duplicated) => {
                        debug!(
                            "duplicated column: block root {root}, column {index}, kept existing verified column",
                            root = id.block_root(),
                            index = id.index()
                        );
                    }
                    Err(err) => {
                        error!("failed to persist verified column: {err}");
                    }
                }
            }
            Err(err) => {
                debug!(
                    "rejected candidate column: block root {root}, column {index}: {err}",
                    root = id.block_root(),
                    index = id.index()
                );
            }
        }
    }
}
