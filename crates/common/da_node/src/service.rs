use std::sync::Arc;

use ream_da::{
    column::CandidateColumn,
    store::{ColumnWriteStore, InsertOutcome},
    verifier::ColumnVerifier,
};
use ream_executor::ReamExecutor;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::ingest::{IngestWorkItem, RetentionHint};

/// Drains the ingest queue, verifying each candidate and persisting those
/// that pass — the single consumer, and the store's only writer.
pub struct DataAvailabilityVerificationService {
    receiver: mpsc::Receiver<IngestWorkItem>,
    verifier: Arc<dyn ColumnVerifier>,
    store: Arc<dyn ColumnWriteStore>,
    executor: ReamExecutor,
}

impl DataAvailabilityVerificationService {
    pub fn new(
        receiver: mpsc::Receiver<IngestWorkItem>,
        verifier: Arc<dyn ColumnVerifier>,
        store: Arc<dyn ColumnWriteStore>,
        executor: ReamExecutor,
    ) -> Self {
        Self {
            receiver,
            verifier,
            store,
            executor,
        }
    }
    /// Consume work items until the ingest channel closes.
    ///
    /// TODO: batching can come once a batchable verifier exists.
    pub async fn run(mut self) {
        info!("DA verification service started");
        while let Some(item) = self.receiver.recv().await {
            match item {
                IngestWorkItem::Candidate(candidate) => self.process_candidate(candidate).await,
                IngestWorkItem::Retention(hint) => self.process_retention(hint).await,
            }
        }
        info!("DA verification service stopped: ingestion queue closed");
    }

    async fn process_retention(&self, hint: RetentionHint) {
        let store = self.store.clone();
        let boundary = hint.slot;
        match self
            .executor
            .spawn_blocking(move || store.prune_below_slot(boundary))
            .await
        {
            Ok(Ok(count)) => {
                if count > 0 {
                    info!("retention pruned {count} column files below slot {boundary}");
                }
            }
            Ok(Err(err)) => error!("retention prune failed: {err}"),
            Err(err) => error!("retention prune worker panicked or was cancelled: {err}"),
        }
    }

    async fn process_candidate(&self, candidate: CandidateColumn) {
        let id = candidate.id;
        let verifier = self.verifier.clone();

        // Skip already-held columns before paying for verification. A failed
        // pre-check must not drop the candidate: fall through and let
        // verify + put decide.
        match self.store.availability(id.block_root()) {
            Ok(availability) if availability.holds(id.index()) => {
                debug!(
                    "skipping already-held column: block root {root}, column {index}",
                    root = id.block_root(),
                    index = id.index()
                );
                return;
            }
            Ok(_) => {}
            Err(err) => debug!("presence pre-check failed, verifying anyway: {err}"),
        }

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
                        warn!(
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

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };

    use alloy_primitives::B256;
    use ream_da::{
        column::{CandidateColumn, ColumnContext, VerifiedColumn},
        error::ValidationError,
        id::ColumnId,
        store::ColumnReadStore,
        verifier::ColumnVerifier,
    };
    use ream_executor::ReamExecutor;

    use super::DataAvailabilityVerificationService;
    use crate::{
        ingest::{RetentionHint, ingest_channel},
        store::FileColumnStore,
    };

    /// Pass-through verifier: these tests exercise the queue-to-store
    /// plumbing, not the cryptography (tested in `ream-da-verifier-kzg`).
    struct AcceptAllVerifier;

    impl ColumnVerifier for AcceptAllVerifier {
        fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError> {
            Ok(VerifiedColumn::new_unchecked(
                candidate.id,
                candidate.context,
                candidate.payload,
            ))
        }
    }

    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("ream-da-pipeline-test-{pid}-{n}"))
    }

    fn sample_candidate(
        block_root: B256,
        index: u64,
        slot: u64,
        payload: &[u8],
    ) -> CandidateColumn {
        CandidateColumn {
            id: ColumnId::new(block_root, index).expect("index within range"),
            context: ColumnContext { slot },
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn submitted_candidates_are_verified_and_stored() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(AcceptAllVerifier);
        let (handle, rx) = ingest_channel(8);
        let service =
            DataAvailabilityVerificationService::new(rx, verifier, store.clone(), executor.clone());

        let candidates = vec![
            sample_candidate(B256::repeat_byte(1), 0, 10, b"col-0"),
            sample_candidate(B256::repeat_byte(1), 7, 10, b"col-7"),
            sample_candidate(B256::repeat_byte(2), 3, 11, b"other-block"),
        ];

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());

            for candidate in &candidates {
                handle.submit(candidate.clone()).await.expect("submit");
            }
            // Dropping the only handle closes the queue, so `run` returns
            // after draining.
            drop(handle);
            service_task.await.expect("service task joined");

            for candidate in &candidates {
                let stored = store
                    .get(&candidate.id)
                    .expect("get succeeds")
                    .expect("column is present");
                assert_eq!(stored.payload(), candidate.payload);
            }
        });

        std::fs::remove_dir_all(&root).ok();
    }

    /// A retention hint submitted after some candidates prunes exactly the
    /// columns below its boundary and leaves newer ones in place — exercising
    /// `submit_retention -> queue -> process_retention -> store.prune_below_slot`.
    #[test]
    fn retention_hint_prunes_columns_below_the_boundary() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(AcceptAllVerifier);
        let (handle, rx) = ingest_channel(8);
        let service =
            DataAvailabilityVerificationService::new(rx, verifier, store.clone(), executor.clone());

        // Two old columns at slot 10, one newer at slot 20.
        let old_a = sample_candidate(B256::repeat_byte(1), 0, 10, b"old-a");
        let old_b = sample_candidate(B256::repeat_byte(1), 7, 10, b"old-b");
        let recent = sample_candidate(B256::repeat_byte(2), 3, 20, b"recent");

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());

            // The queue is FIFO and drained by a single consumer, so a hint
            // submitted after the candidates is applied only once they are stored.
            for candidate in [&old_a, &old_b, &recent] {
                handle.submit(candidate.clone()).await.expect("submit");
            }
            handle
                .submit_retention(RetentionHint { slot: 15 })
                .await
                .expect("submit retention");

            drop(handle);
            service_task.await.expect("service task joined");

            // slot 10 < 15 -> pruned; slot 20 >= 15 -> kept.
            assert_eq!(store.get(&old_a.id).expect("get"), None);
            assert_eq!(store.get(&old_b.id).expect("get"), None);
            assert!(store.get(&recent.id).expect("get").is_some());
        });

        std::fs::remove_dir_all(&root).ok();
    }
}
