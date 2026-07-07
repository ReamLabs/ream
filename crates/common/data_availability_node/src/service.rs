use std::sync::Arc;

use ream_data_availability::{
    column::{CandidateBlock, CandidateColumn},
    store::{ColumnWriteStore, InsertOutcome},
    verifier::ColumnVerifier,
};
use ream_executor::ReamExecutor;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

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
        info!("Data-availability verification service started");
        while let Some(item) = self.receiver.recv().await {
            match item {
                IngestWorkItem::Candidate(candidate) => self.process_candidate(candidate).await,
                IngestWorkItem::CandidateBlock(candidate) => {
                    self.process_candidate_block(candidate).await
                }
                IngestWorkItem::Retention(hint) => self.process_retention(hint).await,
            }
        }
        info!("Data-availability verification service stopped: ingestion queue closed");
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

    /// Verify a whole block's candidate batch and persist every column that passes.
    async fn process_candidate_block(&self, candidate: CandidateBlock) {
        let block_root = candidate.block_root();
        let submitted = candidate.columns_len();

        // One bitmap lookup covers the pre-check for the whole batch. As in
        // the single-column path, a failed pre-check falls through to
        // verification rather than dropping the batch.
        let candidate = match self.store.availability(block_root) {
            Ok(availability) => {
                let (root, context, mut columns) = candidate.into_parts();
                columns.retain(|(index, _)| {
                    let held = availability.holds(*index);
                    if held {
                        trace!(
                            "skipping already-held column: block root {block_root}, column {index}"
                        );
                    }
                    !held
                });
                // Re-sending a stored block (e.g. a retry after a 503) is a
                // normal path, not a fault.
                if columns.is_empty() {
                    debug!("skipping fully-held block batch: block root {block_root}");
                    return;
                }
                // Re-assembly cannot fail
                match CandidateBlock::new(root, context, columns) {
                    Ok(block) => block,
                    Err(err) => {
                        error!("failed to rebuild filtered batch: {err}");
                        return;
                    }
                }
            }
            Err(err) => {
                debug!("presence pre-check failed, verifying the whole batch: {err}");
                candidate
            }
        };

        // One verifier call for the block; adapters may batch the cryptography.
        let verifier = self.verifier.clone();
        let verdicts = match self
            .executor
            .spawn_blocking(move || verifier.verify_block(candidate))
            .await
        {
            Ok(verdicts) => verdicts,
            Err(err) => {
                error!("verification worker panicked or was cancelled: {err}");
                return;
            }
        };

        // Sort the verdicts, and rejects are only counted and logged.
        let mut rejected = 0usize;
        let verified: Vec<_> = verdicts
            .into_iter()
            .filter_map(|(index, verdict)| match verdict {
                Ok(column) => Some(column),
                Err(err) => {
                    rejected += 1;
                    debug!(
                        "rejected candidate column: block root {block_root}, column {index}: {err}"
                    );
                    None
                }
            })
            .collect();

        // Persist all survivors in one storage hop
        let store = self.store.clone();
        let stored = match self
            .executor
            .spawn_blocking(move || {
                let mut stored = 0usize;
                for column in verified {
                    match store.put(column) {
                        Ok(InsertOutcome::Inserted) => stored += 1,
                        Ok(InsertOutcome::Duplicated) => {
                            warn!("duplicated column in batch: block root {block_root}")
                        }
                        Err(err) => error!("failed to persist verified column: {err}"),
                    }
                }
                stored
            })
            .await
        {
            Ok(stored) => stored,
            Err(err) => {
                error!("storage worker panicked or was cancelled: {err}");
                return;
            }
        };

        // One summary line per batch instead of one per column.
        if rejected > 0 {
            warn!(
                "block batch {block_root}: {stored} stored, {rejected} rejected, {submitted} submitted"
            );
        } else {
            debug!("block batch {block_root}: {stored} stored of {submitted} submitted");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, AtomicUsize, Ordering},
        },
    };

    use alloy_primitives::B256;
    use ream_data_availability::{
        column::{CandidateBlock, CandidateColumn, ColumnContext, VerifiedColumn},
        error::ValidationError,
        id::ColumnId,
        store::{ColumnReadStore, ColumnWriteStore},
        verifier::ColumnVerifier,
    };
    use ream_executor::ReamExecutor;

    use super::DataAvailabilityVerificationService;
    use crate::{
        ingest::{RetentionHint, ingest_channel},
        store::FileColumnStore,
    };

    /// Pass-through verifier: these tests exercise the queue-to-store
    /// plumbing, not the cryptography (tested in `ream-data-availability-verifier-kzg`).
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
        std::env::temp_dir().join(format!("ream-data-pipeline-test-{pid}-{n}"))
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

    /// A verifier that counts its calls and rejects a chosen set of column
    /// indices — lets batch tests observe both the pre-filter (skipped columns
    /// are never verified) and per-column verdicts.
    struct CountingVerifier {
        calls: AtomicUsize,
        reject: Vec<u64>,
    }

    impl CountingVerifier {
        fn accepting_all() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                reject: vec![],
            }
        }

        fn rejecting(reject: Vec<u64>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                reject,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl ColumnVerifier for CountingVerifier {
        fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if self.reject.contains(&candidate.id.index()) {
                return Err(ValidationError::InvalidProof);
            }
            Ok(VerifiedColumn::new_unchecked(
                candidate.id,
                candidate.context,
                candidate.payload,
            ))
        }
    }

    fn sample_block(block_root: B256, slot: u64, indices: &[u64]) -> CandidateBlock {
        CandidateBlock::new(
            block_root,
            ColumnContext { slot },
            indices
                .iter()
                .map(|index| (*index, vec![*index as u8]))
                .collect(),
        )
        .expect("a valid batch")
    }

    /// A block batch submitted through the handle is verified and every column
    /// lands in the store — the whole `submit_block -> queue -> verify_block ->
    /// put` path in one go.
    #[test]
    fn submitted_block_batch_is_verified_and_stored() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::accepting_all());
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier.clone(),
            store.clone(),
            executor.clone(),
        );

        let block_root = B256::repeat_byte(5);
        let block = sample_block(block_root, 40, &[0, 7, 127]);

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            handle.submit_block(block).await.expect("submit block");
            drop(handle);
            service_task.await.expect("service task joined");

            for index in [0, 7, 127] {
                let id = ColumnId::new(block_root, index).expect("valid index");
                let stored = store.get(&id).expect("get").expect("column present");
                assert_eq!(stored.payload(), &[index as u8]);
                assert_eq!(stored.context().slot, 40);
            }
            assert_eq!(verifier.calls(), 3, "each submitted column verified once");
        });

        std::fs::remove_dir_all(&root).ok();
    }

    /// Already-held columns are filtered out by one availability lookup before
    /// verification — the verifier never sees them.
    #[test]
    fn block_batch_skips_already_held_columns() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::accepting_all());
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier.clone(),
            store.clone(),
            executor.clone(),
        );

        let block_root = B256::repeat_byte(6);
        // Column 1 is already in the store before the batch arrives.
        store
            .put(VerifiedColumn::new_unchecked(
                ColumnId::new(block_root, 1).expect("valid index"),
                ColumnContext { slot: 40 },
                vec![1],
            ))
            .expect("seed store");

        let block = sample_block(block_root, 40, &[0, 1, 2]);

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            handle.submit_block(block).await.expect("submit block");
            drop(handle);
            service_task.await.expect("service task joined");

            // All three columns are held...
            for index in [0, 1, 2] {
                let id = ColumnId::new(block_root, index).expect("valid index");
                assert!(store.get(&id).expect("get").is_some());
            }
            // ...but the held one was never re-verified.
            assert_eq!(verifier.calls(), 2, "held column skipped verification");
        });

        std::fs::remove_dir_all(&root).ok();
    }

    /// Per-column verdicts: rejected columns are dropped, their siblings are
    /// stored — one bad column never poisons the batch.
    #[test]
    fn block_batch_stores_survivors_when_some_columns_are_rejected() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::rejecting(vec![3]));
        let (handle, rx) = ingest_channel(8);
        let service =
            DataAvailabilityVerificationService::new(rx, verifier, store.clone(), executor.clone());

        let block_root = B256::repeat_byte(8);
        let block = sample_block(block_root, 40, &[2, 3, 4]);

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            handle.submit_block(block).await.expect("submit block");
            drop(handle);
            service_task.await.expect("service task joined");

            // The rejected column is absent; its siblings are stored.
            for index in [2u64, 4] {
                let id = ColumnId::new(block_root, index).expect("valid index");
                assert!(store.get(&id).expect("get").is_some(), "sibling stored");
            }
            let rejected = ColumnId::new(block_root, 3).expect("valid index");
            assert_eq!(store.get(&rejected).expect("get"), None);
        });

        std::fs::remove_dir_all(&root).ok();
    }
}
