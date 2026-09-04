use std::{sync::Arc, time::Duration};

use alloy_primitives::B256;
use ream_data_availability::{
    availability::ColumnAvailability,
    column::{CandidateBlock, CandidateColumn},
    id::ColumnId,
    reconstruction::ColumnReconstructor,
    store::{ColumnWriteStore, InsertOutcome},
    verifier::ColumnVerifier,
};
use ream_executor::ReamExecutor;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

use crate::ingest::{IngestWorkItem, ReconstructionRequest, RetentionHint};

/// Default cap on the settling delay between a block becoming recoverable and
/// the recovery attempt.
pub const DEFAULT_RECONSTRUCTION_DELAY: Duration = Duration::from_secs(3);

/// Drains the ingest queue, verifying each candidate and persisting those
/// that pass — the single consumer, and the store's only writer.
pub struct DataAvailabilityVerificationService {
    receiver: mpsc::Receiver<IngestWorkItem>,
    verifier: Arc<dyn ColumnVerifier>,
    reconstructor: Arc<dyn ColumnReconstructor>,
    store: Arc<dyn ColumnWriteStore>,
    executor: ReamExecutor,
    self_sender: mpsc::WeakSender<IngestWorkItem>,
    max_reconstruction_delay: Duration,
}

impl DataAvailabilityVerificationService {
    pub fn new(
        receiver: mpsc::Receiver<IngestWorkItem>,
        verifier: Arc<dyn ColumnVerifier>,
        reconstructor: Arc<dyn ColumnReconstructor>,
        store: Arc<dyn ColumnWriteStore>,
        executor: ReamExecutor,
        self_sender: mpsc::WeakSender<IngestWorkItem>,
        max_reconstruction_delay: Duration,
    ) -> Self {
        Self {
            receiver,
            verifier,
            reconstructor,
            store,
            executor,
            self_sender,
            max_reconstruction_delay,
        }
    }
    /// Consume work items until the ingest channel closes.
    ///
    /// A single sequential consumer: each item is fully handled before the next
    /// is taken.
    pub async fn run(mut self) {
        info!("Data-availability verification service started");
        while let Some(item) = self.receiver.recv().await {
            match item {
                IngestWorkItem::Candidate(candidate) => self.process_candidate(candidate).await,
                IngestWorkItem::CandidateBlock(candidate) => {
                    self.process_candidate_block(candidate).await
                }
                IngestWorkItem::Retention(hint) => self.process_retention(hint).await,
                IngestWorkItem::Reconstruction(request) => {
                    self.process_reconstruction(request).await
                }
            }
        }
        info!("Data-availability verification service stopped: ingestion queue closed");
    }

    /// Recover a block's missing columns and re-admit them through the normal
    /// verify-then-store path.
    async fn process_reconstruction(&self, request: ReconstructionRequest) {
        let block_root = request.block_root;

        // Re-check: during the delay period, naturally-arriving columns can
        // reach 128 columns and turn this reconstruction into this no-op.
        let availability = self.store.availability(block_root);
        if !availability.is_reconstructable() {
            debug!(
                "skipping reconstruction of block {block_root}: {held} columns held",
                held = availability.held_count()
            );
            return;
        }

        let store = self.store.clone();
        let reconstructor = self.reconstructor.clone();
        match self
            .executor
            .spawn_blocking(move || {
                Self::recover_block(
                    store.as_ref(),
                    reconstructor.as_ref(),
                    block_root,
                    availability,
                )
            })
            .await
        {
            Ok(Some(block)) => self.process_candidate_block(block).await,
            Ok(None) => {} // nothing to admit; the worker logged why
            Err(err) => error!("reconstruction worker panicked or was cancelled: {err}"),
        }
    }

    /// Worker body: fetch the block's held columns and recover the missing ones
    fn recover_block(
        store: &dyn ColumnWriteStore,
        reconstructor: &dyn ColumnReconstructor,
        block_root: B256,
        availability: ColumnAvailability,
    ) -> Option<CandidateBlock> {
        let mut held = Vec::new();
        for index in availability.held_indices() {
            let id = ColumnId::new(block_root, index).expect("held indices are always in range");
            match store.get(&id) {
                Ok(Some(column)) => held.push(column),
                // The index says held but the file is gone or unreadable;
                // recovery still succeeds if at least half remain, so keep
                // collecting.
                Ok(None) => {}
                Err(err) => {
                    warn!("skipping unreadable column {index} of block {block_root}: {err}")
                }
            }
        }
        let context = held.first()?.context();

        let recovered = match reconstructor.reconstruct(held) {
            Ok(recovered) => recovered,
            Err(err) => {
                warn!("reconstruction of block {block_root} failed: {err}");
                return None;
            }
        };
        if recovered.is_empty() {
            debug!("reconstruction of block {block_root} found nothing missing");
            return None;
        }

        let count = recovered.len();
        let columns = recovered
            .into_iter()
            .map(|column| (column.id.index(), column.payload))
            .collect();
        match CandidateBlock::new(block_root, context, columns) {
            Ok(block) => {
                info!(
                    "recovered {count} missing columns for block {block_root}, resubmitting for verification"
                );
                Some(block)
            }
            Err(err) => {
                warn!("recovered columns of block {block_root} form an invalid batch: {err}");
                None
            }
        }
    }

    /// Arm the delayed reconstruction trigger if this write moved the block
    /// *across* the recoverable threshold.
    fn maybe_schedule_reconstruction(&self, block_root: B256, before: &ColumnAvailability) {
        let after = self.store.availability(block_root);
        // Arm only on the crossing: a block that was already recoverable
        // before this work item was armed by an earlier one.
        if before.is_reconstructable() || !after.is_reconstructable() {
            return;
        }

        // Spec: https://ethereum.github.io/consensus-specs/fulu/das-core/#reconstruction-and-cross-seeding
        // Sample the delay fresh per trigger, in [0, cap)
        let delay = self.max_reconstruction_delay.mul_f64(rand::random::<f64>());
        debug!(
            "block {block_root} became recoverable with {held} columns, scheduling reconstruction in {delay:?}",
            held = after.held_count(),
        );
        let sender = self.self_sender.clone();
        self.executor.spawn(async move {
            tokio::time::sleep(delay).await;
            // All real handles gone: shutting down, drop the trigger.
            let Some(sender) = sender.upgrade() else {
                return;
            };
            let _ = sender
                .send(IngestWorkItem::Reconstruction(ReconstructionRequest {
                    block_root,
                }))
                .await;
        });
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

        // `put` is the correctness gate; skipping here only avoids paying
        // verification for a column that is already doomed.
        if self.store.is_below_retention(candidate.context.slot) {
            debug!(
                "refusing column below the retention floor: block root {root}, column {index}, floor {floor}",
                root = id.block_root(),
                index = id.index(),
                floor = self.store.get_retention_floor()
            );
            return;
        }

        // Skip already-held columns before paying for verification. The
        // pre-insert view is kept so the reconstruction trigger can detect a
        // threshold crossing.
        let before = self.store.availability(id.block_root());
        if before.holds(id.index()) {
            debug!(
                "skipping already-held column: block root {root}, column {index}",
                root = id.block_root(),
                index = id.index()
            );
            return;
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
                        self.maybe_schedule_reconstruction(id.block_root(), &before);
                    }
                    Ok(InsertOutcome::Duplicated) => {
                        warn!(
                            "duplicated column: block root {root}, column {index}, kept existing verified column",
                            root = id.block_root(),
                            index = id.index()
                        );
                    }
                    Ok(InsertOutcome::BelowRetention) => {
                        debug!(
                            "refused column below the retention floor: block root {root}, column {index}, floor {floor}",
                            root = id.block_root(),
                            index = id.index(),
                            floor = self.store.get_retention_floor()
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

        // Same floor pre-check as the single-column path; one slot covers the
        // whole block, so this skips up to a block's worth of verification.
        if self.store.is_below_retention(candidate.context().slot) {
            debug!(
                "refusing block below the retention floor: block root {block_root}, floor {floor}",
                floor = self.store.get_retention_floor()
            );
            return;
        }

        // The pre-insert view is kept so the reconstruction trigger can
        // detect a threshold crossing.
        let before = self.store.availability(block_root);
        let (root, context, mut columns) = candidate.into_parts();
        columns.retain(|(index, _)| {
            let held = before.holds(*index);
            if held {
                trace!("skipping already-held column: block root {block_root}, column {index}");
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
        let candidate = match CandidateBlock::new(root, context, columns) {
            Ok(block) => block,
            Err(err) => {
                error!("failed to rebuild filtered batch: {err}");
                return;
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
                        Ok(InsertOutcome::BelowRetention) => {
                            debug!(
                                "refused column below the retention floor: block root {block_root}, floor {floor}",
                                floor = store.get_retention_floor()
                            )
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

        if stored > 0 {
            self.maybe_schedule_reconstruction(block_root, &before);
        }

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
        time::Duration,
    };

    use alloy_primitives::B256;
    use ream_data_availability::{
        column::{CandidateBlock, CandidateColumn, ColumnContext, VerifiedColumn},
        error::ValidationError,
        id::{ColumnId, NUMBER_OF_COLUMNS},
        reconstruction::ColumnReconstructor,
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

    /// Reconstruction double for tests that exercise other paths: loud if the
    /// pipeline ever invokes it unexpectedly.
    struct PanicReconstructor;

    impl ColumnReconstructor for PanicReconstructor {
        fn reconstruct(
            &self,
            _held: Vec<VerifiedColumn>,
        ) -> Result<Vec<CandidateColumn>, ValidationError> {
            panic!("reconstruct must not be called in this test");
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
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier,
            Arc::new(PanicReconstructor),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
        );

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
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier,
            Arc::new(PanicReconstructor),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
        );

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

    /// A verifier that counts its calls and rejects a chosen set of column indices
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
            Arc::new(PanicReconstructor),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
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
            Arc::new(PanicReconstructor),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
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

    #[test]
    fn block_batch_stores_survivors_when_some_columns_are_rejected() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::rejecting(vec![3]));
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier,
            Arc::new(PanicReconstructor),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
        );

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

    #[test]
    fn below_floor_candidate_is_skipped_before_verification() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::accepting_all());
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier.clone(),
            Arc::new(PanicReconstructor),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
        );

        // The floor is already at 100 when the candidate arrives.
        store.prune_below_slot(100).expect("raise the floor");
        let stale = sample_candidate(B256::repeat_byte(4), 2, 10, b"stale");

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            handle.submit(stale.clone()).await.expect("submit");
            drop(handle);
            service_task.await.expect("service task joined");

            assert_eq!(store.get(&stale.id).expect("get"), None, "not stored");
            assert_eq!(
                verifier.calls(),
                0,
                "a below-floor candidate must be skipped before verification"
            );
        });

        std::fs::remove_dir_all(&root).ok();
    }

    struct FillMissingReconstructor {
        calls: AtomicUsize,
    }

    impl FillMissingReconstructor {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl ColumnReconstructor for FillMissingReconstructor {
        fn reconstruct(
            &self,
            held: Vec<VerifiedColumn>,
        ) -> Result<Vec<CandidateColumn>, ValidationError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let first = held.first().expect("the service never sends an empty set");
            let (block_root, context) = (first.id().block_root(), first.context());
            let mut held_mask = 0u128;
            for column in &held {
                held_mask |= 1 << column.id().index();
            }
            Ok((0..NUMBER_OF_COLUMNS)
                .filter(|index| held_mask & (1 << index) == 0)
                .map(|index| CandidateColumn {
                    id: ColumnId::new(block_root, index).expect("index within range"),
                    context,
                    payload: vec![index as u8],
                })
                .collect())
        }
    }

    /// Poll until the block holds every column or a deadline passes.
    async fn wait_until_complete(store: &FileColumnStore, block_root: B256) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let held = store.availability(block_root).held_count();
            if held == NUMBER_OF_COLUMNS {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "block did not self-heal in time ({held} columns held)"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[test]
    fn incomplete_block_self_heals_through_the_verify_gate() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::accepting_all());
        let reconstructor = FillMissingReconstructor::new();
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier.clone(),
            reconstructor.clone(),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
        );

        let block_root = B256::repeat_byte(9);
        let first_half: Vec<u64> = (0..NUMBER_OF_COLUMNS / 2).collect();
        let block = sample_block(block_root, 40, &first_half);

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            handle.submit_block(block).await.expect("submit block");

            // The self-heal round-trips through the queue, so the handle must
            // stay alive (it backs the service's weak self-sender) until the
            // block is complete.
            wait_until_complete(&store, block_root).await;
            drop(handle);
            service_task.await.expect("service task joined");

            assert_eq!(reconstructor.calls(), 1, "one crossing, one recovery");
            assert_eq!(
                verifier.calls(),
                NUMBER_OF_COLUMNS as usize,
                "64 ingested + 64 recovered columns all verified"
            );
            let recovered = ColumnId::new(block_root, 100).expect("valid index");
            let stored = store.get(&recovered).expect("get").expect("present");
            assert_eq!(stored.payload(), &[100u8]);
            assert_eq!(stored.context().slot, 40);
        });

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_trickled_column_crossing_the_threshold_triggers_recovery() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::accepting_all());
        let reconstructor = FillMissingReconstructor::new();
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier,
            reconstructor.clone(),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
        );

        // 63 columns are already held; the 64th arrives as a single candidate.
        let block_root = B256::repeat_byte(10);
        for index in 0..NUMBER_OF_COLUMNS / 2 - 1 {
            store
                .put(VerifiedColumn::new_unchecked(
                    ColumnId::new(block_root, index).expect("valid index"),
                    ColumnContext { slot: 40 },
                    vec![index as u8],
                ))
                .expect("seed store");
        }
        let crossing = sample_candidate(block_root, NUMBER_OF_COLUMNS / 2 - 1, 40, b"cross");

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            handle.submit(crossing).await.expect("submit");

            wait_until_complete(&store, block_root).await;
            drop(handle);
            service_task.await.expect("service task joined");

            assert_eq!(reconstructor.calls(), 1, "one crossing, one recovery");
        });

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reconstruction_stands_down_when_the_block_completes_naturally() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::accepting_all());
        let reconstructor = FillMissingReconstructor::new();
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier,
            reconstructor.clone(),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::from_millis(300),
        );

        let block_root = B256::repeat_byte(11);
        let first_half: Vec<u64> = (0..NUMBER_OF_COLUMNS / 2).collect();
        let second_half: Vec<u64> = (NUMBER_OF_COLUMNS / 2..NUMBER_OF_COLUMNS).collect();

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            // The first half arms the trigger; the second half completes the
            // block well inside the 300ms delay.
            handle
                .submit_block(sample_block(block_root, 40, &first_half))
                .await
                .expect("submit first half");
            handle
                .submit_block(sample_block(block_root, 40, &second_half))
                .await
                .expect("submit second half");

            // Wait past the delay so the trigger has fired and re-checked.
            tokio::time::sleep(Duration::from_millis(900)).await;
            drop(handle);
            service_task.await.expect("service task joined");

            assert_eq!(
                reconstructor.calls(),
                0,
                "a complete block is never recovered"
            );
        });

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_reconstruction_below_half_the_columns() {
        let executor = ReamExecutor::new().expect("create executor");
        let root = temp_root();
        let store = Arc::new(FileColumnStore::new(root.clone()).expect("open store"));
        let verifier = Arc::new(CountingVerifier::accepting_all());
        let reconstructor = FillMissingReconstructor::new();
        let (handle, rx) = ingest_channel(8);
        let service = DataAvailabilityVerificationService::new(
            rx,
            verifier,
            reconstructor.clone(),
            store.clone(),
            executor.clone(),
            handle.downgrade(),
            Duration::ZERO,
        );

        let block_root = B256::repeat_byte(12);
        let below_half: Vec<u64> = (0..NUMBER_OF_COLUMNS / 2 - 1).collect();

        executor.runtime().block_on(async move {
            let service_task = tokio::spawn(service.run());
            handle
                .submit_block(sample_block(block_root, 40, &below_half))
                .await
                .expect("submit block");

            // With a zero delay an armed trigger would fire immediately; give
            // it ample room to prove it never does.
            tokio::time::sleep(Duration::from_millis(300)).await;
            drop(handle);
            service_task.await.expect("service task joined");

            assert_eq!(
                reconstructor.calls(),
                0,
                "below half there is nothing to arm"
            );
        });

        std::fs::remove_dir_all(&root).ok();
    }
}
