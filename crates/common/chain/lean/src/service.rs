use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use alloy_primitives::B256;
use anyhow::anyhow;
use futures::stream::{FuturesUnordered, StreamExt};
use libp2p_identity::PeerId;
use libp2p_swarm::ConnectionId;
use rand::seq::IndexedRandom;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAttestation},
    block::{BlockWithSignatures, SignedBlock},
    checkpoint::Checkpoint,
};
use ream_consensus_misc::constants::lean::{ATTESTATION_COMMITTEE_COUNT, INTERVALS_PER_SLOT};
use ream_fork_choice_lean::store::LeanStoreWriter;
use ream_metrics::{
    ATTESTATION_COMMITTEE_COUNT as ATTESTATION_COMMITTEE_COUNT_METRIC, CURRENT_SLOT, IS_AGGREGATOR,
    set_int_gauge_vec,
};
use ream_network_spec::networks::lean_network_spec;
use ream_network_state_lean::NetworkState;
use ream_req_resp::lean::{
    NetworkEvent, ResponseCallback,
    messages::{LeanRequestMessage, LeanResponseMessage},
};
use ream_storage::tables::{field::REDBField, table::REDBTable};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{Instrument, Level, debug, enabled, error, info, trace, warn};
use tree_hash::TreeHash;

use crate::{
    clock::{create_lean_clock_interval, get_initial_tick_count},
    messages::{LeanChainServiceMessage, ServiceResponse},
    p2p_request::{LeanP2PRequest, P2PCallbackRequest},
    slot::get_current_slot,
    sync::{
        BackfillState, QueueRecovery, SyncStatus,
        forward_background_syncer::{ForwardBackgroundSyncer, ForwardSyncResults},
        job::{pending::PendingJobRequest, request::JobRequest},
        strategy::{
            BackfillTimeoutStrategy, HandoffInputs, HandoffStrategy, NearHeadBackfillStrategy,
            NearHeadFanoutStrategy, PeerSelectionStrategy, PendingRequestDedupStrategy,
            should_fanout_near_head, should_switch_to_synced,
        },
    },
};

const STATE_RETENTION_SLOTS: u64 = 128;
const NEAR_HEAD_BRIDGE_MAX_GAP_SLOTS: u64 = 3;
const NEAR_HEAD_FANOUT_MAX_GAP_SLOTS: u64 = 4;
const RECENT_SYNC_BLOCK_RETENTION: Duration = Duration::from_secs(16);
const BACKFILL_PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(2);
const BACKFILL_HEDGE_DELAY: Duration = Duration::from_millis(250);
const BACKFILL_QUEUE_STALL_TIMEOUT_FLOOR: Duration = Duration::from_secs(30);
const BACKFILL_QUEUE_STALL_TIMEOUT_MULTIPLIER: f64 = 8.0;
const SYNCED_BACKFILL_GAP_PERSISTENCE_THRESHOLD: Duration = Duration::from_millis(300);
const MAX_BACKFILL_RECOVERY_ATTEMPTS: u32 = 8;
const NO_SEEDABLE_BACKFILL_CHECKPOINT_WARN_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncBlockSource {
    ReqResp,
    Gossip,
}

#[derive(Debug, Clone, Copy)]
struct RecentSyncBlock {
    parent_root: B256,
    slot: u64,
    seen_at: Instant,
    source: SyncBlockSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallbackLossMode {
    None,
    DropFirstPerRoot,
}

impl CallbackLossMode {
    fn from_env() -> Self {
        match std::env::var("REAM_LEAN_PACKET_LOSS_MODE") {
            Ok(value) if value.eq_ignore_ascii_case("drop-first-per-root") => {
                Self::DropFirstPerRoot
            }
            _ => Self::None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct BackfillTelemetry {
    requests_sent: u64,
    request_retries: u64,
    callbacks_processed: u64,
    callbacks_dropped: u64,
    callback_latency_ms_total: u128,
    callback_latency_samples: u64,
}

#[derive(Debug)]
struct SyncTelemetry {
    near_head_backfill_strategy: NearHeadBackfillStrategy,
    near_head_fanout_strategy: NearHeadFanoutStrategy,
    handoff_strategy: HandoffStrategy,
    backfill_timeout_strategy: BackfillTimeoutStrategy,
    pending_dedup_strategy: PendingRequestDedupStrategy,
    peer_selection_strategy: PeerSelectionStrategy,
    recent_sync_blocks: Vec<RecentSyncBlock>,
    callback_loss_mode: CallbackLossMode,
    dropped_callback_roots: HashSet<B256>,
    backfill_telemetry: BackfillTelemetry,
    last_backfill_progress_log: Option<Instant>,
    last_no_seedable_backfill_checkpoint_warn: Option<Instant>,
    synced_peer_gap_started_at: Option<Instant>,
    inflight_roots: HashMap<B256, InflightRootRequest>,
    peer_avg_latency_ms: HashMap<PeerId, f64>,
}

impl SyncTelemetry {
    fn from_env() -> Self {
        Self {
            near_head_backfill_strategy: NearHeadBackfillStrategy::from_env(),
            near_head_fanout_strategy: NearHeadFanoutStrategy::from_env(),
            handoff_strategy: HandoffStrategy::from_env(),
            backfill_timeout_strategy: BackfillTimeoutStrategy::from_env(),
            pending_dedup_strategy: PendingRequestDedupStrategy::from_env(),
            peer_selection_strategy: PeerSelectionStrategy::from_env(),
            recent_sync_blocks: Vec::new(),
            callback_loss_mode: CallbackLossMode::from_env(),
            dropped_callback_roots: HashSet::new(),
            backfill_telemetry: BackfillTelemetry::default(),
            last_backfill_progress_log: None,
            last_no_seedable_backfill_checkpoint_warn: None,
            synced_peer_gap_started_at: None,
            inflight_roots: HashMap::new(),
            peer_avg_latency_ms: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct InflightRootRequest {
    primary_peer: PeerId,
    backup_peer: Option<PeerId>,
    requested_at: Instant,
    backup_sent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryCheckpointAction {
    Queued {
        attempts: u32,
    },
    AlreadyQueued,
    AlreadyQuarantined {
        attempts: u32,
        current_head_slot: u64,
        network_finalized_slot: u64,
    },
    AlreadyDropped,
    QuarantinedByBudget {
        attempts: u32,
        current_head_slot: u64,
        network_finalized_slot: u64,
    },
    DroppedByBudget {
        attempts: u32,
        current_head_slot: u64,
        network_finalized_slot: u64,
    },
}

enum BackfillParentResolution {
    Complete {
        completion_root: B256,
    },
    NeedsRequest {
        request_slot: u64,
        missing_root: B256,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuarantinedBackfillRoot {
    slot: u64,
    attempts: u32,
}

#[derive(Debug, Clone, Copy)]
struct CheckpointLocalCoverage {
    local_head: B256,
    current_head_slot: u64,
    checkpoint_in_block_store: bool,
    checkpoint_in_pending_store: bool,
}

type CallbackFuture = Pin<
    Box<
        dyn Future<
                Output = (
                    Option<ResponseCallback>,
                    tokio::sync::mpsc::Receiver<ResponseCallback>,
                ),
            > + Send
            + Sync
            + 'static,
    >,
>;

/// LeanChainService is responsible for updating the [LeanChain] state
pub struct LeanChainService {
    store: Arc<LeanStoreWriter>,
    receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
    outbound_p2p: mpsc::UnboundedSender<LeanP2PRequest>,
    network_state: Arc<NetworkState>,
    sync_status: SyncStatus,
    backfill_state: BackfillState,
    peers_in_use: HashSet<PeerId>,
    pending_job_requests: VecDeque<PendingJobRequest>,
    forward_syncer: Option<JoinHandle<anyhow::Result<ForwardSyncResults>>>,
    checkpoints_to_queue: Vec<(Checkpoint, bool)>,
    backfill_recovery_attempts: HashMap<B256, u32>,
    quarantined_backfill_roots: HashMap<B256, QuarantinedBackfillRoot>,
    dropped_backfill_roots: HashSet<B256>,
    pending_callbacks: FuturesUnordered<CallbackFuture>,
    is_aggregator: bool,
    telemetry: SyncTelemetry,
}

impl LeanChainService {
    pub async fn new(
        store: LeanStoreWriter,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        outbound_p2p: mpsc::UnboundedSender<LeanP2PRequest>,
        is_aggregator: bool,
    ) -> Self {
        let network_state = store.read().await.network_state.clone();
        LeanChainService {
            network_state,
            store: Arc::new(store),
            receiver,
            outbound_p2p,
            sync_status: SyncStatus::Syncing,
            backfill_state: BackfillState::default(),
            peers_in_use: HashSet::new(),
            forward_syncer: None,
            checkpoints_to_queue: Vec::new(),
            backfill_recovery_attempts: HashMap::new(),
            quarantined_backfill_roots: HashMap::new(),
            dropped_backfill_roots: HashSet::new(),
            pending_callbacks: FuturesUnordered::new(),
            pending_job_requests: VecDeque::new(),
            is_aggregator,
            telemetry: SyncTelemetry::from_env(),
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        set_int_gauge_vec(&IS_AGGREGATOR, self.is_aggregator as i64, &[]);
        set_int_gauge_vec(
            &ATTESTATION_COMMITTEE_COUNT_METRIC,
            ATTESTATION_COMMITTEE_COUNT as i64,
            &[],
        );

        info!(
            genesis_time = lean_network_spec().genesis_time,
            near_head_backfill_strategy = ?self.telemetry.near_head_backfill_strategy,
            near_head_fanout_strategy = ?self.telemetry.near_head_fanout_strategy,
            handoff_strategy = ?self.telemetry.handoff_strategy,
            backfill_timeout_strategy = ?self.telemetry.backfill_timeout_strategy,
            pending_dedup_strategy = ?self.telemetry.pending_dedup_strategy,
            peer_selection_strategy = ?self.telemetry.peer_selection_strategy,
            callback_loss_mode = ?self.telemetry.callback_loss_mode,
            "LeanChainService started",
        );

        let mut tick_count = get_initial_tick_count();

        info!("LeanChainService starting at tick_count: {}", tick_count);

        let mut interval = create_lean_clock_interval()
            .map_err(|err| anyhow!("Expected Ream to be started before genesis time: {err:?}"))?;

        let mut sync_interval = tokio::time::interval(Duration::from_millis(50));
        sync_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut genesis_passed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            >= lean_network_spec().genesis_time;

        loop {
            if !genesis_passed
                && SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    >= lean_network_spec().genesis_time
            {
                genesis_passed = true;
            }

            tokio::select! {
                _ = interval.tick() => {
                    if tick_count.is_multiple_of(INTERVALS_PER_SLOT) {
                        self.sync_status = self.update_sync_status().await?;
                    }
                    if self.sync_status == SyncStatus::Synced {
                        let tick_start = Instant::now();
                        self.store.write().await.tick_interval(tick_count.is_multiple_of(INTERVALS_PER_SLOT), self.is_aggregator).await.expect("Failed to tick interval");
                        let tick_elapsed = tick_start.elapsed();
                        if tick_elapsed > Duration::from_millis(200) {
                            info!(
                                tick_count,
                                interval = tick_count % INTERVALS_PER_SLOT,
                                slot = tick_count / INTERVALS_PER_SLOT,
                                is_aggregator = self.is_aggregator,
                                elapsed_ms = tick_elapsed.as_millis() as u64,
                                "DIAG tick_interval slow",
                            );
                        }
                        self.step_head_sync(tick_count).await?;
                    }

                    tick_count += 1;
                }
                _ = sync_interval.tick(), if genesis_passed => {
                    if self.sync_status != SyncStatus::Synced || self.should_run_backfill_sync().await {
                        self.step_backfill_sync().await?;
                    }
                }
                forward_syncer = async {
                    if let Some(handle) = self.forward_syncer.as_mut() {
                        handle.await
                    } else {
                        std::future::pending().await
                    }
                }, if self.forward_syncer.is_some() => {
                    let forward_syncer = match forward_syncer {
                        Ok(forward_syncer) => forward_syncer,
                        Err(err) => {
                            error!("Forward background sync JoinHandle error: {err:?}");
                            continue;
                        },
                    };
                    self.forward_syncer = None;

                    let forward_syncer = match forward_syncer {
                        Ok(forward_syncer) => forward_syncer,
                        Err(err) => {
                            error!("Forward background sync failed: {err:?}");
                            continue;
                        },
                    };

                    self.handle_forward_sync_result(forward_syncer).await?;
                }
                Some((callback_response, rx)) = self.pending_callbacks.next() => {
                    match callback_response {
                        Some(ResponseCallback::ResponseMessage { peer_id, message, .. }) => {
                            self.handle_callback_response_message(peer_id, message).await?;
                            self.push_callback_receiver(rx);
                        }
                        Some(ResponseCallback::EndOfStream {peer_id,  request_id }) => {
                            trace!("Received end of stream for request_id {request_id} from peer {peer_id:?}");
                        }
                        Some(ResponseCallback::NotConnected { peer_id }) => {
                            warn!("Received NotConnected callback for peer {peer_id:?}");
                            self.handle_failed_job_request(peer_id).await?;
                        }
                        None => {
                            warn!("Callback channel closed unexpectedly");
                        }
                    }
                }
                Some(message) = self.receiver.recv() => {
                    match message {
                        LeanChainServiceMessage::ProduceBlock { slot, sender } => {
                            if self.sync_status == SyncStatus::Syncing {
                                warn!("Received ProduceBlock request while syncing. Ignoring.");
                                if let Err(err) = sender.send(ServiceResponse::Syncing) {
                                    warn!("Failed to send syncing response for ProduceBlock: {err:?}");
                                }
                                continue;
                            }


                            if let Err(err) = self.handle_produce_block(slot, sender).await {
                                error!("Failed to handle produce block message: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::BuildAttestationData { slot, sender } => {
                            if self.sync_status == SyncStatus::Syncing {
                                warn!("Received BuildAttestationData request while syncing. Ignoring.");
                                if let Err(err) = sender.send(ServiceResponse::Syncing) {
                                    warn!("Failed to send syncing response for BuildAttestationData: {err:?}");
                                }
                                continue;
                            }

                            if let Err(err) = self.handle_build_attestation_data(slot, sender).await {
                                error!("Failed to handle build attestation data message: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::ProcessBlock { signed_block, need_gossip } => {
                            if self.sync_status != SyncStatus::Synced {
                                if let Err(err) = self
                                    .handle_syncing_process_block(&signed_block)
                                    .await
                                {
                                    warn!(
                                        "Failed to handle ProcessBlock while backfill syncing: {err:?}"
                                    );
                                }
                                continue;
                            }

                            if enabled!(Level::DEBUG) {
                                debug!(
                                    slot = signed_block.block.slot,
                                    block_root = ?signed_block.block.tree_hash_root(),
                                    parent_root = ?signed_block.block.parent_root,
                                    state_root = ?signed_block.block.state_root,
                                    attestations_length = signed_block.block.body.attestations.len(),
                                    "Processing block built by Validator {}",
                                    signed_block.block.proposer_index,
                                );
                            } else {
                                info!(
                                    slot = signed_block.block.slot,
                                    block_root = ?signed_block.block.tree_hash_root(),
                                    "Processing block built by Validator {}",
                                    signed_block.block.proposer_index,
                                );
                            }

                            if let Err(err) = self.handle_process_block(&signed_block).await {
                                warn!("Failed to handle process block message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_p2p.send(LeanP2PRequest::GossipBlock(signed_block)) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }

                        LeanChainServiceMessage::ProcessAttestation { signed_attestation, subnet_id, need_gossip } => {
                            if self.sync_status != SyncStatus::Synced {
                                trace!("Received ProcessAttestation request while syncing. Ignoring.");
                                continue;
                            }

                            debug!(
                                slot = signed_attestation.message.slot,
                                head = ?signed_attestation.message.head,
                                source = ?signed_attestation.message.source,
                                target = ?signed_attestation.message.target,
                                subnet_id,
                                "Processing attestation by Validator {}",
                                signed_attestation.validator_id,
                            );

                            if let Err(err) = self.handle_process_attestation(*signed_attestation.clone()).await {
                                warn!("Failed to handle process attestation message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_p2p.send(LeanP2PRequest::GossipAttestation { subnet_id, attestation: signed_attestation }) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }

                        LeanChainServiceMessage::ProcessAggregatedAttestation { aggregated_attestation, need_gossip } => {
                            if self.sync_status != SyncStatus::Synced {
                                trace!("Received ProcessAggregatedAttestation request while syncing. Ignoring.");
                                continue;
                            }

                            debug!(aggregated_attestation.data.slot, "Processing aggregated attestation");

                            if let Err(err) = self.store.write().await.on_gossip_aggregated_attestation(*aggregated_attestation.clone()).await {
                                warn!("Failed to handle process aggregated attestation message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_p2p.send(LeanP2PRequest::GossipAggregatedAttestation(aggregated_attestation)) {
                                warn!("Failed to send aggregated attestation to outbound gossip channel: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::CheckIfCanonicalCheckpoint { peer_id, checkpoint, sender } => {
                            let slot_index_provider = self.store.read().await.store.lock().await.slot_index_provider();
                            let is_canonical = match slot_index_provider.get(checkpoint.slot)  {
                                Ok(Some(block_root)) => block_root == checkpoint.root,
                                Ok(None) => true,
                                Err(err) => {
                                    warn!("Failed to get slot index for checkpoint: {err:?}");
                                    false
                                }
                            };

                            // Special case: Genesis checkpoint is always canonical.
                            let is_canonical = if checkpoint.slot < 5 {
                                true
                            } else {
                                is_canonical
                            };

                            if let Err(err) = sender.send((peer_id, is_canonical)) {
                                warn!("Failed to send canonical checkpoint response: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::GetBlocksByRoot { roots, sender } => {
                            let block_provider = {
                                let fork_choice = self.store.read().await;
                                let store = fork_choice.store.lock().await;
                                store.block_provider()
                            };
                            let mut blocks = Vec::with_capacity(roots.len());

                            for root in roots {
                                match block_provider.get(root) {
                                    Ok(Some(block)) => blocks.push(Arc::new(block)),
                                    Ok(None) => {
                                        debug!("Block not found for root: {root:?}");
                                    }
                                    Err(err) => {
                                        warn!("LeanChainServiceMessage::GetBlocksByRoot: Failed to get block for root {root:?}: {err:?}");
                                    }
                                }
                            }

                            if let Err(err) = sender.send(blocks) {
                                warn!("Failed to send blocks by root response: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::NetworkEvent(event) => {
                            if let Err(err) = self.handle_network_event(event).await {
                                warn!("Failed to handle network event: {err:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    async fn step_head_sync(&mut self, tick_count: u64) -> anyhow::Result<()> {
        let interval = tick_count % INTERVALS_PER_SLOT;
        match interval {
            0 => {
                // First tick: Log current head state, including its
                // justification/finalization status.
                let (head, state_provider) = {
                    let fork_choice = self.store.read().await;
                    let store = fork_choice.store.lock().await;
                    (store.head_provider().get()?, store.state_provider())
                };
                let head_state = state_provider
                    .get(head)?
                    .ok_or_else(|| anyhow!("Post state not found for head: {head}"))?;

                set_int_gauge_vec(&CURRENT_SLOT, head_state.slot as i64, &[]);

                info!(
                    "\n\
                            ============================================================\n\
                            REAM's CHAIN STATUS: Next Slot: {current_slot} | Head Slot: {head_slot}\n\
                            ------------------------------------------------------------\n\
                            Connected Peers:   {connected_peer_count}\n\
                            ------------------------------------------------------------\n\
                            Head Block Root:   {head_block_root}\n\
                            Parent Block Root: {parent_block_root}\n\
                            State Root:        {state_root}\n\
                            ------------------------------------------------------------\n\
                            Latest Justified:  Slot {justified_slot} | Root: {justified_root}\n\
                            Latest Finalized:  Slot {finalized_slot} | Root: {finalized_root}\n\
                            ============================================================",
                    current_slot = get_current_slot(),
                    head_slot = head_state.slot,
                    connected_peer_count = self.network_state.connected_peer_count(),
                    head_block_root = head.to_string(),
                    parent_block_root = head_state.latest_block_header.parent_root,
                    state_root = head_state.tree_hash_root(),
                    justified_slot = head_state.latest_justified.slot,
                    justified_root = head_state.latest_justified.root,
                    finalized_slot = head_state.latest_finalized.slot,
                    finalized_root = head_state.latest_finalized.root,
                );
            }
            1 => {
                // Second tick: Prune old data.
                if let Err(err) = self.prune_old_state(tick_count).await {
                    warn!("Pruning cycle failed (non-fatal): {err:?}");
                }
            }
            3 => {
                // Fourth tick: Compute the safe target.
                info!(
                    slot = get_current_slot(),
                    tick = tick_count,
                    "Computing safe target"
                );
                self.store
                    .write()
                    .await
                    .update_safe_target()
                    .await
                    .expect("Failed to update safe target");
            }
            4 => {
                // Fifth tick: Accept new attestations.
                info!(
                    slot = get_current_slot(),
                    tick = tick_count,
                    "Accepting new attestations"
                );
                self.store
                    .write()
                    .await
                    .accept_new_attestations()
                    .await
                    .expect("Failed to accept new attestations");
            }
            _ => {
                // Other ticks: Do nothing.
            }
        }
        Ok(())
    }

    async fn step_backfill_sync(&mut self) -> anyhow::Result<()> {
        self.refresh_quarantined_backfill_roots().await?;
        self.prune_stale_pending_blocks().await?;
        self.maybe_log_backfill_progress();
        self.prune_recent_sync_blocks();
        let backfill_job_timeout = self.current_backfill_job_timeout().await;
        for timed_out_job in self
            .backfill_state
            .reset_timed_out_jobs(backfill_job_timeout)
        {
            self.telemetry.backfill_telemetry.request_retries += 1;
            self.telemetry.inflight_roots.remove(&timed_out_job.root);
            warn!(
                peer_id = ?timed_out_job.peer_id,
                root = ?timed_out_job.root,
                timeout_seconds = backfill_job_timeout.as_secs_f64(),
                "Backfill job request timed out; scheduling peer reassignment"
            );
            self.network_state
                .failed_response_from_peer(timed_out_job.peer_id);
            self.peers_in_use.remove(&timed_out_job.peer_id);
            self.queue_pending_reset(timed_out_job.peer_id);
        }
        let stalled_queue_timeout = self.current_backfill_queue_stall_timeout(backfill_job_timeout);
        for recovery in self
            .backfill_state
            .recover_stalled_queues(stalled_queue_timeout)
        {
            self.recover_stalled_queue(recovery, stalled_queue_timeout)
                .await?;
        }

        // If a queue has reached the stored head, execute that queue in a background thread,
        // blocking any other threads from processing until it returns. The thread can
        // return early and start a new queue if it finds that It can't walk back to the stored
        // head.
        if self.forward_syncer.is_none()
            && let Some(earliest_complete_queue) = self.backfill_state.get_ready_to_process_queue()
        {
            let store = self.store.clone();
            let network_state = self.network_state.clone();
            info!(
                sync_status = ?self.sync_status,
                queue_starting_root = ?earliest_complete_queue.starting_root,
                queue_starting_slot = earliest_complete_queue.starting_slot,
                queue_count = self.backfill_state.jobs.len(),
                "Starting forward background sync for completed queue",
            );
            self.forward_syncer = Some(tokio::spawn(
                async move {
                    let mut forward_syncer =
                        ForwardBackgroundSyncer::new(store, network_state, earliest_complete_queue);
                    forward_syncer.start().await
                }
                .in_current_span(),
            ));
        }

        // queue unqueued jobs
        let peer_gap_slots = self.current_peer_gap_slots().await;
        let enable_near_head_fanout = should_fanout_near_head(
            self.telemetry.near_head_fanout_strategy,
            peer_gap_slots,
            NEAR_HEAD_FANOUT_MAX_GAP_SLOTS,
        );
        self.queue_pending_job_requests().await?;
        self.process_delayed_hedges();
        let unqueued_jobs = self.backfill_state.unqueued_jobs();
        for job in unqueued_jobs {
            if self.pending_job_requests.iter().any(|request| {
                matches!(
                    request,
                    PendingJobRequest::Reset {
                        peer_id: existing_peer_id
                    } if *existing_peer_id == job.peer_id
                )
            }) {
                trace!(
                    peer_id = ?job.peer_id,
                    root = ?job.root,
                    "Skipping unqueued job while peer reset remains pending"
                );
                continue;
            }

            if matches!(
                self.telemetry.near_head_backfill_strategy,
                NearHeadBackfillStrategy::GossipPreferred
            ) && self.try_advance_job_with_cached_block(job.root).await?
            {
                continue;
            }

            if self.telemetry.inflight_roots.contains_key(&job.root) {
                continue;
            }

            let backup_peer = if enable_near_head_fanout {
                self.alternate_peer_for_fanout(job.peer_id)
            } else {
                None
            };

            if !self.request_block_by_root_from_peer(job.peer_id, job.root) {
                continue;
            }

            let mut inflight_request = InflightRootRequest {
                primary_peer: job.peer_id,
                backup_peer,
                requested_at: Instant::now(),
                backup_sent: false,
            };
            if self.telemetry.near_head_fanout_strategy == NearHeadFanoutStrategy::DualPeer
                && let Some(backup_peer_id) = backup_peer
            {
                inflight_request.backup_sent =
                    self.request_block_by_root_from_peer(backup_peer_id, job.root);
                if inflight_request.backup_sent {
                    trace!(
                        root = ?job.root,
                        primary_peer_id = ?job.peer_id,
                        backup_peer_id = ?backup_peer_id,
                        "Fanout backfill request sent to backup peer"
                    );
                }
            }
            self.telemetry
                .inflight_roots
                .insert(job.root, inflight_request);

            self.backfill_state.mark_job_as_requested(job.root);
        }

        self.queue_pending_job_requests().await?;
        self.process_staged_backfill_checkpoints().await?;

        // start new queue from peers status
        let preferred_highest_checkpoint = match self.preferred_peer_head_checkpoint() {
            Some(checkpoint) => checkpoint,
            None => {
                self.maybe_warn_no_seedable_backfill_checkpoint();
                return Ok(());
            }
        };

        let coverage = self
            .checkpoint_local_coverage(preferred_highest_checkpoint)
            .await?;
        let checkpoint_already_buffered_for_existing_backfill =
            self.checkpoint_already_buffered_for_existing_backfill(coverage, false);

        if self.should_skip_backfill_checkpoint(preferred_highest_checkpoint, coverage, false) {
            trace!(
                root = ?preferred_highest_checkpoint.root,
                slot = preferred_highest_checkpoint.slot,
                local_head = ?coverage.local_head,
                current_head_slot = coverage.current_head_slot,
                checkpoint_in_block_store = coverage.checkpoint_in_block_store,
                checkpoint_in_pending_store = coverage.checkpoint_in_pending_store,
                checkpoint_already_buffered_for_existing_backfill,
                sync_status = ?self.sync_status,
                "Skipping backfill queue because peer checkpoint is already covered by local chain or pending backfill state"
            );
            return Ok(());
        }

        if self
            .dropped_backfill_roots
            .contains(&preferred_highest_checkpoint.root)
        {
            trace!(
                root = ?preferred_highest_checkpoint.root,
                slot = preferred_highest_checkpoint.slot,
                current_head_slot = coverage.current_head_slot,
                sync_status = ?self.sync_status,
                "Skipping backfill queue because checkpoint root has exhausted recovery budget"
            );
            return Ok(());
        }

        if self
            .quarantined_backfill_roots
            .contains_key(&preferred_highest_checkpoint.root)
        {
            trace!(
                root = ?preferred_highest_checkpoint.root,
                slot = preferred_highest_checkpoint.slot,
                current_head_slot = coverage.current_head_slot,
                sync_status = ?self.sync_status,
                "Skipping backfill queue because checkpoint root is quarantined pending finalization or block arrival"
            );
            return Ok(());
        }

        if self
            .backfill_state
            .slot_is_subset_of_any_queue(preferred_highest_checkpoint.slot)
            || self
                .checkpoints_to_queue
                .iter()
                .any(|(checkpoint, _)| checkpoint.slot == preferred_highest_checkpoint.slot)
        {
            return Ok(());
        }

        self.checkpoints_to_queue
            .push((preferred_highest_checkpoint, false));
        self.process_staged_backfill_checkpoints().await
    }

    fn choose_weighted_peer(&self, candidates: &[(PeerId, u8)]) -> Option<PeerId> {
        match candidates.choose_weighted(&mut rand::rng(), |(peer_id, score)| {
            self.peer_weight(*peer_id, *score)
        }) {
            Ok((peer_id, _)) => Some(*peer_id),
            Err(err) => {
                if !candidates.is_empty() {
                    warn!("Failed to choose weighted peer: {err}");
                }
                None
            }
        }
    }

    fn choose_assignable_peer_from_candidates(
        &self,
        candidates: &[(PeerId, u8)],
        avoid_peer_id: Option<PeerId>,
    ) -> Option<PeerId> {
        let preferred_candidates: Vec<(PeerId, u8)> = candidates
            .iter()
            .copied()
            .filter(|(peer_id, _)| {
                !self.peers_in_use.contains(peer_id) && Some(*peer_id) != avoid_peer_id
            })
            .collect();

        if let Some(peer_id) = self.choose_weighted_peer(&preferred_candidates) {
            return Some(peer_id);
        }

        let fallback_candidates: Vec<(PeerId, u8)> = candidates
            .iter()
            .copied()
            .filter(|(peer_id, _)| Some(*peer_id) != avoid_peer_id)
            .collect();

        if !fallback_candidates.is_empty() {
            debug!(
                peers_in_use = self.peers_in_use.len(),
                fallback_candidates = fallback_candidates.len(),
                avoid_peer_id = ?avoid_peer_id,
                "Falling back to assigning work to a connected peer already marked in use"
            );
        }

        self.choose_weighted_peer(&fallback_candidates)
    }

    async fn assignable_peer_id(&self, avoid_peer_id: Option<PeerId>) -> Option<PeerId> {
        let connected_peers = self.network_state.connected_peer_ids_with_scores();
        self.choose_assignable_peer_from_candidates(&connected_peers, avoid_peer_id)
    }

    async fn assignable_peer_id_for_slot(
        &self,
        min_head_slot: u64,
        avoid_peer_id: Option<PeerId>,
    ) -> Option<PeerId> {
        let slot_candidates = self
            .network_state
            .connected_peer_ids_with_scores_at_or_above_slot(min_head_slot);
        if let Some(peer_id) =
            self.choose_assignable_peer_from_candidates(&slot_candidates, avoid_peer_id)
        {
            return Some(peer_id);
        }

        self.assignable_peer_id(avoid_peer_id).await
    }

    async fn assignable_peer_id_for_checkpoint(
        &self,
        checkpoint: Checkpoint,
        avoid_peer_id: Option<PeerId>,
    ) -> Option<PeerId> {
        let checkpoint_candidates = self
            .network_state
            .connected_peer_ids_with_scores_matching_head(checkpoint);
        if let Some(peer_id) =
            self.choose_assignable_peer_from_candidates(&checkpoint_candidates, avoid_peer_id)
        {
            return Some(peer_id);
        }

        self.assignable_peer_id_for_slot(checkpoint.slot, avoid_peer_id)
            .await
    }

    fn alternate_peer_for_fanout(&self, primary_peer_id: PeerId) -> Option<PeerId> {
        let candidates: Vec<(PeerId, u8)> = self
            .network_state
            .connected_peer_ids_with_scores()
            .into_iter()
            .filter(|(peer_id, _)| *peer_id != primary_peer_id)
            .collect();

        self.choose_weighted_peer(&candidates)
    }

    fn peer_weight(&self, peer_id: PeerId, score: u8) -> f64 {
        let score_weight = f64::from(score.max(1));
        match self.telemetry.peer_selection_strategy {
            PeerSelectionStrategy::ScoreOnly => score_weight,
            PeerSelectionStrategy::LatencyWeighted => {
                let latency_penalty = self
                    .telemetry
                    .peer_avg_latency_ms
                    .get(&peer_id)
                    .map(|latency_ms| 1.0 / (1.0 + (latency_ms / 1500.0)))
                    .unwrap_or(1.0);
                (score_weight * latency_penalty).max(0.1)
            }
        }
    }

    fn request_block_by_root_from_peer(&mut self, peer_id: PeerId, root: B256) -> bool {
        let (callback, rx) = mpsc::channel(100);
        if let Err(err) = self.outbound_p2p.send(LeanP2PRequest::Request {
            peer_id,
            callback,
            message: P2PCallbackRequest::BlocksByRoot { roots: vec![root] },
        }) {
            warn!(
                "Failed to send block request to peer {:?} for root {:?}: {err:?}",
                peer_id, root
            );
            self.network_state.failed_response_from_peer(peer_id);
            return false;
        }
        self.push_callback_receiver(rx);
        self.telemetry.backfill_telemetry.requests_sent += 1;
        true
    }

    fn process_delayed_hedges(&mut self) {
        if self.telemetry.near_head_fanout_strategy != NearHeadFanoutStrategy::DelayedHedge {
            return;
        }

        let now = Instant::now();
        let roots_to_hedge: Vec<(B256, PeerId, PeerId)> = self
            .telemetry
            .inflight_roots
            .iter()
            .filter_map(|(root, inflight)| {
                let backup_peer = inflight.backup_peer?;
                if inflight.backup_sent
                    || now.saturating_duration_since(inflight.requested_at) < BACKFILL_HEDGE_DELAY
                {
                    return None;
                }
                Some((*root, inflight.primary_peer, backup_peer))
            })
            .collect();

        for (root, primary_peer_id, backup_peer_id) in roots_to_hedge {
            let backup_sent = self.request_block_by_root_from_peer(backup_peer_id, root);
            if backup_sent {
                if let Some(inflight) = self.telemetry.inflight_roots.get_mut(&root) {
                    inflight.backup_sent = true;
                }
                trace!(
                    root = ?root,
                    primary_peer_id = ?primary_peer_id,
                    backup_peer_id = ?backup_peer_id,
                    "Delayed hedge backfill request sent to backup peer"
                );
            }
        }
    }

    async fn current_peer_gap_slots(&self) -> u64 {
        let local_head_slot = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            let head = match store.head_provider().get() {
                Ok(head) => head,
                Err(_) => return 0,
            };
            match store.block_provider().get(head) {
                Ok(Some(block)) => block.block.slot,
                _ => return 0,
            }
        };
        let highest_peer_head_slot = self
            .preferred_peer_head_checkpoint()
            .map(|checkpoint| checkpoint.slot)
            .unwrap_or(local_head_slot);
        highest_peer_head_slot.saturating_sub(local_head_slot)
    }

    async fn update_sync_status(&mut self) -> anyhow::Result<SyncStatus> {
        if self.forward_syncer.is_some() {
            return Ok(self.sync_status);
        }

        let (head, block_provider, has_orphaned_pending_blocks) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (
                store.head_provider().get()?,
                store.block_provider(),
                !store.pending_blocks_provider().is_empty(),
            )
        };
        let current_head_slot = block_provider
            .get(head)?
            .ok_or_else(|| anyhow!("Block not found for head: {head}"))?
            .block
            .slot;

        let tolerance = std::cmp::max(8, (lean_network_spec().num_validators * 2) / 3);
        let highest_peer_head_slot = self
            .preferred_peer_head_checkpoint()
            .map(|c| c.slot)
            .unwrap_or(current_head_slot);
        let highest_peer_finalized_slot = self
            .network_state
            .common_finalized_checkpoint()
            .map(|c| c.slot)
            .unwrap_or(current_head_slot);
        let is_synced_by_time = get_current_slot() <= current_head_slot + tolerance;
        let is_behind_finalized = highest_peer_finalized_slot > current_head_slot;
        let has_pending_backfill_work = self.has_pending_backfill_work();
        let has_active_backfill_jobs = self.has_active_backfill_jobs();
        let has_inflight_backfill_requests = !self.telemetry.inflight_roots.is_empty();
        let has_near_head_bridge = self.has_recent_near_head_gossip_bridge(
            head,
            current_head_slot,
            highest_peer_head_slot,
        );
        let should_transition_to_synced = should_switch_to_synced(
            self.telemetry.handoff_strategy,
            HandoffInputs {
                is_behind_peers: is_behind_finalized,
                has_orphaned_pending_blocks,
                has_pending_backfill_work,
                has_near_head_bridge,
                has_active_backfill_jobs,
                has_inflight_backfill_requests,
            },
        );
        let transitioned_to_synced =
            should_transition_to_synced && self.sync_status != SyncStatus::Synced;

        let sync_status = if self.sync_status == SyncStatus::Synced {
            if is_behind_finalized {
                info!(
                    slot = get_current_slot(),
                    head_slot = current_head_slot,
                    peer_head_slot = highest_peer_head_slot,
                    peer_finalized_slot = highest_peer_finalized_slot,
                    has_orphaned_pending_blocks,
                    has_pending_backfill_work,
                    has_near_head_bridge,
                    has_active_backfill_jobs,
                    has_inflight_backfill_requests,
                    handoff_strategy = ?self.telemetry.handoff_strategy,
                    "Node fell behind network finalized checkpoint; switching to backfill syncing mode"
                );
                SyncStatus::Syncing
            } else {
                SyncStatus::Synced
            }
        } else if should_transition_to_synced {
            if transitioned_to_synced {
                if is_synced_by_time {
                    info!(
                        slot = get_current_slot(),
                        head_slot = current_head_slot,
                        peer_finalized_slot = highest_peer_finalized_slot,
                        "Node has synced to the head"
                    );
                } else {
                    info!(
                        slot = get_current_slot(),
                        head_slot = current_head_slot,
                        peer_finalized_slot = highest_peer_finalized_slot,
                        "Node is behind time but caught up to finalized safety; switching to Synced"
                    );
                }
            }
            SyncStatus::Synced
        } else {
            self.sync_status
        };

        if transitioned_to_synced {
            self.telemetry.synced_peer_gap_started_at = None;
            self.telemetry.dropped_callback_roots.clear();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|err| anyhow!("System time before unix epoch: {err:?}"))?
                .as_secs();
            self.store.write().await.on_tick(now, false, true).await?;
        } else if sync_status != SyncStatus::Synced {
            self.telemetry.synced_peer_gap_started_at = None;
        }

        Ok(sync_status)
    }

    async fn should_run_backfill_sync(&mut self) -> bool {
        let has_orphaned_pending_blocks = self.has_orphaned_pending_blocks().await;
        let has_pending_backfill_work = self.has_pending_backfill_work();
        let has_inflight_backfill_requests = !self.telemetry.inflight_roots.is_empty();
        let peer_gap_slots = self.current_peer_gap_slots().await;

        if self.sync_status != SyncStatus::Synced {
            return peer_gap_slots > 0
                || has_orphaned_pending_blocks
                || has_pending_backfill_work
                || has_inflight_backfill_requests;
        }

        if has_orphaned_pending_blocks
            || has_pending_backfill_work
            || has_inflight_backfill_requests
        {
            self.telemetry.synced_peer_gap_started_at = None;
            return true;
        }

        if peer_gap_slots == 0 {
            self.telemetry.synced_peer_gap_started_at = None;
            return false;
        }

        let gap_started_at = self
            .telemetry
            .synced_peer_gap_started_at
            .get_or_insert_with(Instant::now);
        gap_started_at.elapsed() >= SYNCED_BACKFILL_GAP_PERSISTENCE_THRESHOLD
    }

    async fn current_head_and_finalized_slots(&self) -> anyhow::Result<(u64, u64)> {
        let current_head_slot = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            let head = store.head_provider().get()?;
            let block_provider = store.block_provider();
            block_provider
                .get(head)?
                .ok_or_else(|| anyhow!("Block not found for head: {head}"))?
                .block
                .slot
        };
        let network_finalized_slot = self
            .network_state
            .common_finalized_checkpoint()
            .map(|checkpoint| checkpoint.slot)
            .unwrap_or(current_head_slot);

        Ok((current_head_slot, network_finalized_slot))
    }

    async fn checkpoint_local_coverage(
        &self,
        checkpoint: Checkpoint,
    ) -> anyhow::Result<CheckpointLocalCoverage> {
        let fork_choice = self.store.read().await;
        let store = fork_choice.store.lock().await;
        let local_head = store.head_provider().get()?;
        let block_provider = store.block_provider();
        let pending_blocks_provider = store.pending_blocks_provider();
        let current_head_slot = block_provider
            .get(local_head)?
            .ok_or_else(|| anyhow!("Block not found for head: {local_head}"))?
            .block
            .slot;

        Ok(CheckpointLocalCoverage {
            local_head,
            current_head_slot,
            checkpoint_in_block_store: block_provider.get(checkpoint.root)?.is_some(),
            checkpoint_in_pending_store: pending_blocks_provider.get(checkpoint.root)?.is_some(),
        })
    }

    fn checkpoint_already_buffered_for_existing_backfill(
        &self,
        coverage: CheckpointLocalCoverage,
        bypass_slot_check: bool,
    ) -> bool {
        if bypass_slot_check || !coverage.checkpoint_in_pending_store {
            return false;
        }

        !self.backfill_state.jobs.is_empty()
            || !self.pending_job_requests.is_empty()
            || !self.peers_in_use.is_empty()
            || self.forward_syncer.is_some()
            || !self.telemetry.inflight_roots.is_empty()
    }

    fn should_skip_backfill_checkpoint(
        &self,
        checkpoint: Checkpoint,
        coverage: CheckpointLocalCoverage,
        bypass_slot_check: bool,
    ) -> bool {
        checkpoint.root == coverage.local_head
            || coverage.checkpoint_in_block_store
            || (!bypass_slot_check && checkpoint.slot <= coverage.current_head_slot)
            || self.checkpoint_already_buffered_for_existing_backfill(coverage, bypass_slot_check)
    }

    async fn process_staged_backfill_checkpoints(&mut self) -> anyhow::Result<()> {
        let current_preferred_highest_checkpoint = self.preferred_peer_head_checkpoint();

        while let Some((checkpoint, bypass_slot_check)) = self.checkpoints_to_queue.pop() {
            if !bypass_slot_check && current_preferred_highest_checkpoint != Some(checkpoint) {
                debug!(
                    queue_root = ?checkpoint.root,
                    queue_slot = checkpoint.slot,
                    current_preferred_highest_checkpoint = ?current_preferred_highest_checkpoint,
                    sync_status = ?self.sync_status,
                    "Dropping staged backfill checkpoint because it no longer matches the current preferred peer checkpoint",
                );
                continue;
            }
            if self
                .quarantined_backfill_roots
                .contains_key(&checkpoint.root)
            {
                debug!(
                    queue_root = ?checkpoint.root,
                    queue_slot = checkpoint.slot,
                    bypass_slot_check,
                    sync_status = ?self.sync_status,
                    "Skipping staged backfill checkpoint because the root is quarantined",
                );
                continue;
            }
            if self.dropped_backfill_roots.contains(&checkpoint.root) {
                debug!(
                    queue_root = ?checkpoint.root,
                    queue_slot = checkpoint.slot,
                    bypass_slot_check,
                    sync_status = ?self.sync_status,
                    "Skipping staged backfill checkpoint because the root has exhausted recovery budget",
                );
                continue;
            }

            let coverage = self.checkpoint_local_coverage(checkpoint).await?;
            let checkpoint_already_buffered_for_existing_backfill =
                self.checkpoint_already_buffered_for_existing_backfill(coverage, bypass_slot_check);
            if self.should_skip_backfill_checkpoint(checkpoint, coverage, bypass_slot_check) {
                debug!(
                    queue_root = ?checkpoint.root,
                    queue_slot = checkpoint.slot,
                    bypass_slot_check,
                    local_head = ?coverage.local_head,
                    current_head_slot = coverage.current_head_slot,
                    checkpoint_in_block_store = coverage.checkpoint_in_block_store,
                    checkpoint_in_pending_store = coverage.checkpoint_in_pending_store,
                    checkpoint_already_buffered_for_existing_backfill,
                    sync_status = ?self.sync_status,
                    "Dropping staged backfill checkpoint because it is already covered by local chain or pending backfill state",
                );
                continue;
            }

            let non_queued_peer_id = match self
                .assignable_peer_id_for_checkpoint(checkpoint, None)
                .await
            {
                Some(id) => id,
                None => {
                    if self.network_state.connected_peer_count() == 0 {
                        info!(
                            queue_root = ?checkpoint.root,
                            queue_slot = checkpoint.slot,
                            bypass_slot_check,
                            sync_status = ?self.sync_status,
                            "No connected peers available to start backfill queue",
                        );
                    } else {
                        info!(
                            queue_root = ?checkpoint.root,
                            queue_slot = checkpoint.slot,
                            bypass_slot_check,
                            connected_peer_count = self.network_state.connected_peer_count(),
                            peers_in_use = self.peers_in_use.len(),
                            sync_status = ?self.sync_status,
                            "Unable to start backfill queue because all connected peers are already assigned",
                        );
                    }
                    self.checkpoints_to_queue
                        .push((checkpoint, bypass_slot_check));
                    return Ok(());
                }
            };
            let new_queue_added = self.backfill_state.add_new_job_queue(
                checkpoint,
                JobRequest::new(non_queued_peer_id, checkpoint.root),
                bypass_slot_check,
            );
            if new_queue_added {
                self.peers_in_use.insert(non_queued_peer_id);
                info!(
                    queue_root = ?checkpoint.root,
                    queue_slot = checkpoint.slot,
                    assigned_peer_id = ?non_queued_peer_id,
                    bypass_slot_check,
                    sync_status = ?self.sync_status,
                    queue_count = self.backfill_state.jobs.len(),
                    "Started backfill queue",
                );
            } else {
                debug!(
                    queue_root = ?checkpoint.root,
                    queue_slot = checkpoint.slot,
                    bypass_slot_check,
                    sync_status = ?self.sync_status,
                    existing_queue_count = self.backfill_state.jobs.len(),
                    "Skipped backfill queue because an equal-or-better queue already exists",
                );
            }
        }

        Ok(())
    }

    fn preferred_peer_head_checkpoint(&self) -> Option<Checkpoint> {
        self.network_state.preferred_highest_checkpoint()
    }

    fn maybe_warn_no_seedable_backfill_checkpoint(&mut self) {
        let now = Instant::now();
        if let Some(last_warn) = self.telemetry.last_no_seedable_backfill_checkpoint_warn
            && now.saturating_duration_since(last_warn)
                < NO_SEEDABLE_BACKFILL_CHECKPOINT_WARN_INTERVAL
        {
            return;
        }

        self.telemetry.last_no_seedable_backfill_checkpoint_warn = Some(now);

        warn!(
            sync_status = ?self.sync_status,
            connected_peer_count = self.network_state.connected_peer_count(),
            queue_count = self.backfill_state.jobs.len(),
            staged_checkpoints = self.checkpoints_to_queue.len(),
            common_finalized_checkpoint = ?self.network_state.common_finalized_checkpoint(),
            "No common highest checkpoint found among connected peers; cannot seed backfill queue",
        );
    }

    fn clear_backfill_retry_state(&mut self, root: B256) {
        self.backfill_recovery_attempts.remove(&root);
        self.quarantined_backfill_roots.remove(&root);
        self.dropped_backfill_roots.remove(&root);
    }

    fn clear_backfill_arrival_state(&mut self, root: B256) {
        self.quarantined_backfill_roots.remove(&root);
        self.dropped_backfill_roots.remove(&root);
    }

    fn is_suppressed_backfill_root(&self, root: &B256) -> bool {
        self.dropped_backfill_roots.contains(root)
            || self.quarantined_backfill_roots.contains_key(root)
    }

    async fn resolve_backfill_parent_resolution(
        &self,
        head: B256,
        parent_root: B256,
        child_slot: u64,
    ) -> anyhow::Result<BackfillParentResolution> {
        let (pending_blocks_provider, block_provider) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (store.pending_blocks_provider(), store.block_provider())
        };

        let mut current_root = parent_root;
        let mut request_slot = child_slot;

        loop {
            if current_root == B256::ZERO || current_root == head {
                return Ok(BackfillParentResolution::Complete {
                    completion_root: current_root,
                });
            }

            if self.is_suppressed_backfill_root(&current_root) {
                return Ok(BackfillParentResolution::NeedsRequest {
                    request_slot,
                    missing_root: current_root,
                });
            }

            if block_provider.get(current_root)?.is_some() {
                return Ok(BackfillParentResolution::Complete {
                    completion_root: current_root,
                });
            }

            let Some(block) = pending_blocks_provider.get(current_root)? else {
                return Ok(BackfillParentResolution::NeedsRequest {
                    request_slot,
                    missing_root: current_root,
                });
            };

            request_slot = pending_block_slot(&block);
            current_root = pending_block_parent_root(&block);
        }
    }

    async fn refresh_quarantined_backfill_roots(&mut self) -> anyhow::Result<()> {
        if self.quarantined_backfill_roots.is_empty() {
            return Ok(());
        }

        let (_, network_finalized_slot) = self.current_head_and_finalized_slots().await?;
        let roots_to_drop: Vec<(B256, QuarantinedBackfillRoot)> = self
            .quarantined_backfill_roots
            .iter()
            .filter(|(_, entry)| entry.slot <= network_finalized_slot)
            .map(|(root, entry)| (*root, *entry))
            .collect();

        for (root, entry) in roots_to_drop {
            self.quarantined_backfill_roots.remove(&root);
            self.backfill_recovery_attempts.remove(&root);
            self.dropped_backfill_roots.insert(root);
            warn!(
                root = ?root,
                slot = entry.slot,
                attempts = entry.attempts,
                network_finalized_slot,
                "Dropping quarantined backfill root after network finalized passed it",
            );
        }

        Ok(())
    }

    async fn stage_recovery_checkpoint(
        &mut self,
        checkpoint: Checkpoint,
    ) -> anyhow::Result<RecoveryCheckpointAction> {
        self.refresh_quarantined_backfill_roots().await?;

        if self.dropped_backfill_roots.contains(&checkpoint.root) {
            return Ok(RecoveryCheckpointAction::AlreadyDropped);
        }

        if let Some(quarantined_root) = self.quarantined_backfill_roots.get(&checkpoint.root) {
            let (current_head_slot, network_finalized_slot) =
                self.current_head_and_finalized_slots().await?;
            return Ok(RecoveryCheckpointAction::AlreadyQuarantined {
                attempts: quarantined_root.attempts,
                current_head_slot,
                network_finalized_slot,
            });
        }

        if self
            .backfill_state
            .jobs
            .iter()
            .any(|queue| queue.starting_root == checkpoint.root)
            || self
                .checkpoints_to_queue
                .iter()
                .any(|(existing, _)| existing.root == checkpoint.root)
        {
            return Ok(RecoveryCheckpointAction::AlreadyQueued);
        }

        let attempts = {
            let attempts = self
                .backfill_recovery_attempts
                .entry(checkpoint.root)
                .or_insert(0);
            *attempts += 1;
            *attempts
        };

        if attempts > MAX_BACKFILL_RECOVERY_ATTEMPTS {
            let (current_head_slot, network_finalized_slot) =
                self.current_head_and_finalized_slots().await?;
            self.backfill_recovery_attempts.remove(&checkpoint.root);
            if checkpoint.slot > network_finalized_slot {
                self.quarantined_backfill_roots.insert(
                    checkpoint.root,
                    QuarantinedBackfillRoot {
                        slot: checkpoint.slot,
                        attempts,
                    },
                );
                return Ok(RecoveryCheckpointAction::QuarantinedByBudget {
                    attempts,
                    current_head_slot,
                    network_finalized_slot,
                });
            }
            self.dropped_backfill_roots.insert(checkpoint.root);
            return Ok(RecoveryCheckpointAction::DroppedByBudget {
                attempts,
                current_head_slot,
                network_finalized_slot,
            });
        }

        self.checkpoints_to_queue.push((checkpoint, true));
        Ok(RecoveryCheckpointAction::Queued { attempts })
    }

    async fn handle_forward_sync_result(
        &mut self,
        forward_syncer: ForwardSyncResults,
    ) -> anyhow::Result<()> {
        match forward_syncer {
            ForwardSyncResults::Completed {
                starting_root,
                ending_root,
                imported_start_slot,
                imported_end_slot,
                blocks_synced,
                processing_time_seconds,
            } => {
                info!(
                    starting_root = ?starting_root,
                    ending_root = ?ending_root,
                    imported_start_slot,
                    imported_end_slot,
                    blocks_synced,
                    processing_time_seconds,
                    "Forward background sync completed",
                );
                self.clear_backfill_retry_state(ending_root);
                self.backfill_state.remove_processed_queue(ending_root);
            }
            ForwardSyncResults::ChainIncomplete {
                prevous_queue,
                checkpoint_for_new_queue,
            } => {
                self.backfill_state
                    .remove_processed_queue(prevous_queue.starting_root);
                self.parse_recovery_checkpoint(
                    checkpoint_for_new_queue,
                    prevous_queue.starting_root,
                    prevous_queue.starting_slot,
                    None,
                    "Forward background sync incomplete",
                )
                .await?;
            }
            ForwardSyncResults::RootMismatch {
                previous_queue,
                checkpoint_for_new_queue,
                bad_root,
                bad_slot,
                actual_root,
                network_finalized_slot,
            } => {
                let removed_pending_block = self.remove_pending_block(bad_root).await?;
                self.backfill_state
                    .remove_processed_queue(previous_queue.starting_root);

                if let Some(checkpoint_for_new_queue) = checkpoint_for_new_queue {
                    warn!(
                        starting_root = ?previous_queue.starting_root,
                        starting_slot = previous_queue.starting_slot,
                        bad_root = ?bad_root,
                        bad_slot,
                        actual_root = ?actual_root,
                        network_finalized_slot,
                        restart_root = ?checkpoint_for_new_queue.root,
                        restart_slot = checkpoint_for_new_queue.slot,
                        removed_pending_block,
                        "Forward background sync root mismatch; purged bad pending block and evaluating root recovery",
                    );
                    self.parse_recovery_checkpoint(
                        checkpoint_for_new_queue,
                        previous_queue.starting_root,
                        previous_queue.starting_slot,
                        None,
                        "Forward background sync root mismatch",
                    )
                    .await?;
                } else {
                    self.dropped_backfill_roots.insert(bad_root);
                    warn!(
                        starting_root = ?previous_queue.starting_root,
                        starting_slot = previous_queue.starting_slot,
                        bad_root = ?bad_root,
                        bad_slot,
                        actual_root = ?actual_root,
                        network_finalized_slot,
                        removed_pending_block,
                        "Forward background sync root mismatch for finalized slot; dropping obsolete queue and suppressing the bad root until a fresh block arrives",
                    );
                }
            }
        }

        Ok(())
    }

    async fn has_orphaned_pending_blocks(&self) -> bool {
        let fork_choice = self.store.read().await;
        let store = fork_choice.store.lock().await;
        !store.pending_blocks_provider().is_empty()
    }

    async fn remove_pending_block(&self, root: B256) -> anyhow::Result<bool> {
        let pending_blocks_provider = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            store.pending_blocks_provider()
        };

        Ok(pending_blocks_provider.remove(root)?.is_some())
    }

    async fn prune_stale_pending_blocks(&self) -> anyhow::Result<()> {
        let protected_roots: HashSet<_> = self
            .backfill_state
            .jobs
            .iter()
            .flat_map(|queue| {
                std::iter::once(queue.starting_root).chain(queue.jobs.keys().copied())
            })
            .chain(
                self.checkpoints_to_queue
                    .iter()
                    .map(|(checkpoint, _)| checkpoint.root),
            )
            .chain(self.telemetry.inflight_roots.keys().copied())
            .chain(
                self.pending_job_requests
                    .iter()
                    .filter_map(|request| match request {
                        PendingJobRequest::Initial { root, .. } => Some(*root),
                        PendingJobRequest::Reset { .. } => None,
                    }),
            )
            .collect();
        let (pending_blocks_provider, block_provider, latest_finalized_slot) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (
                store.pending_blocks_provider(),
                store.block_provider(),
                store.latest_finalized_provider().get()?.slot,
            )
        };

        let stale_roots: Vec<_> = pending_blocks_provider
            .iter()?
            .filter_map(|result| match result {
                Ok((root, block)) => {
                    let block_slot = pending_block_slot(&block);
                    let should_prune = !protected_roots.contains(&root)
                        && (block_provider.contains_key(root)
                            || block_slot <= latest_finalized_slot);
                    should_prune.then_some(Ok(root))
                }
                Err(err) => Some(Err(err)),
            })
            .collect::<Result<_, _>>()?;

        if stale_roots.is_empty() {
            return Ok(());
        }

        for root in &stale_roots {
            let _ = pending_blocks_provider.remove(*root)?;
        }

        debug!(
            pruned_pending_blocks = stale_roots.len(),
            latest_finalized_slot, "Pruned stale pending blocks"
        );

        Ok(())
    }

    async fn current_backfill_job_timeout(&self) -> Duration {
        let peer_gap = self.current_peer_gap_slots().await;
        self.telemetry
            .backfill_timeout_strategy
            .timeout_for_peer_gap(peer_gap)
    }

    fn current_backfill_queue_stall_timeout(&self, backfill_job_timeout: Duration) -> Duration {
        backfill_job_timeout
            .mul_f64(BACKFILL_QUEUE_STALL_TIMEOUT_MULTIPLIER)
            .max(BACKFILL_QUEUE_STALL_TIMEOUT_FLOOR)
    }

    fn has_pending_backfill_work(&self) -> bool {
        let has_queued_jobs = !self.backfill_state.jobs.is_empty();
        let has_busy_peers = has_queued_jobs && !self.peers_in_use.is_empty();
        has_queued_jobs
            || !self.pending_job_requests.is_empty()
            || !self.checkpoints_to_queue.is_empty()
            || has_busy_peers
            || self.forward_syncer.is_some()
    }

    fn has_active_backfill_jobs(&self) -> bool {
        self.backfill_state
            .jobs
            .iter()
            .any(|queue| !queue.jobs.is_empty())
    }

    fn sync_queue_stats(&self) -> (usize, usize) {
        let queue_count = self.backfill_state.jobs.len();
        let total_jobs = self
            .backfill_state
            .jobs
            .iter()
            .map(|queue| queue.jobs.len())
            .sum();
        (queue_count, total_jobs)
    }

    fn backfill_queue_progress_summary(&self) -> String {
        if self.backfill_state.jobs.is_empty() {
            return "none".to_string();
        }

        self.backfill_state
            .jobs
            .iter()
            .map(|queue| {
                let mut waiting_slots: Vec<_> = queue
                    .jobs
                    .keys()
                    .map(|root| {
                        if *root == queue.starting_root {
                            queue.starting_slot
                        } else {
                            queue.last_fetched_slot.saturating_sub(1)
                        }
                    })
                    .collect();
                waiting_slots.sort_unstable();
                waiting_slots.dedup();

                let no_progress_yet = !queue.is_complete
                    && queue.jobs.contains_key(&queue.starting_root)
                    && waiting_slots.first().copied() == Some(queue.starting_slot)
                    && waiting_slots.last().copied() == Some(queue.starting_slot);
                let fetched_through = (!no_progress_yet).then_some(queue.last_fetched_slot);
                let waiting_on = match (
                    waiting_slots.first().copied(),
                    waiting_slots.last().copied(),
                ) {
                    (Some(start), Some(end)) if start == end => Some(start.to_string()),
                    (Some(start), Some(end)) => Some(format!("{start}..{end}")),
                    _ => None,
                };

                match (queue.is_complete, fetched_through, waiting_on.as_deref()) {
                    (true, Some(fetched_through), _) => format!(
                        "complete(start={starting_slot}, fetched_through={fetched_through}, jobs={jobs})",
                        starting_slot = queue.starting_slot,
                        jobs = queue.jobs.len()
                    ),
                    (false, Some(fetched_through), Some(waiting_on)) => format!(
                        "active(start={starting_slot}, fetched_through={fetched_through}, waiting_on={waiting_on}, jobs={jobs})",
                        starting_slot = queue.starting_slot,
                        jobs = queue.jobs.len()
                    ),
                    (false, None, Some(waiting_on)) => format!(
                        "active(start={starting_slot}, fetched_through=none, waiting_on={waiting_on}, jobs={jobs})",
                        starting_slot = queue.starting_slot,
                        jobs = queue.jobs.len()
                    ),
                    _ => format!(
                        "queue(start={starting_slot}, fetched_through={fetched_through:?}, waiting_on={waiting_on:?}, jobs={jobs}, complete={is_complete})",
                        starting_slot = queue.starting_slot,
                        jobs = queue.jobs.len(),
                        is_complete = queue.is_complete
                    ),
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn maybe_log_backfill_progress(&mut self) {
        let now = Instant::now();
        if let Some(last_log_time) = self.telemetry.last_backfill_progress_log
            && now.saturating_duration_since(last_log_time) < BACKFILL_PROGRESS_LOG_INTERVAL
        {
            return;
        }
        self.telemetry.last_backfill_progress_log = Some(now);

        let (queue_count, total_jobs) = self.sync_queue_stats();
        let avg_callback_latency_ms =
            if self.telemetry.backfill_telemetry.callback_latency_samples == 0 {
                0.0
            } else {
                self.telemetry.backfill_telemetry.callback_latency_ms_total as f64
                    / self.telemetry.backfill_telemetry.callback_latency_samples as f64
            };
        let queue_progress = self.backfill_queue_progress_summary();
        info!(
            slot = get_current_slot(),
            sync_status = ?self.sync_status,
            queue_count,
            total_jobs,
            staged_checkpoints = self.checkpoints_to_queue.len(),
            pending_requests = self.pending_job_requests.len(),
            inflight_roots = self.telemetry.inflight_roots.len(),
            peers_in_use = self.peers_in_use.len(),
            recent_sync_blocks = self.telemetry.recent_sync_blocks.len(),
            requests_sent = self.telemetry.backfill_telemetry.requests_sent,
            request_retries = self.telemetry.backfill_telemetry.request_retries,
            callbacks_processed = self.telemetry.backfill_telemetry.callbacks_processed,
            callbacks_dropped = self.telemetry.backfill_telemetry.callbacks_dropped,
            avg_callback_latency_ms,
            peer_latency_entries = self.telemetry.peer_avg_latency_ms.len(),
            queue_progress = %queue_progress,
            "Backfill progress"
        );
    }

    fn queue_pending_reset(&mut self, peer_id: PeerId) {
        if self.telemetry.pending_dedup_strategy == PendingRequestDedupStrategy::Dedup
            && self
                .pending_job_requests
                .iter()
                .any(|request| matches!(request, PendingJobRequest::Reset { peer_id: existing_peer_id } if *existing_peer_id == peer_id))
        {
            return;
        }

        self.pending_job_requests
            .push_back(PendingJobRequest::new_reset(peer_id));
    }

    fn queue_pending_initial(&mut self, root: B256, slot: u64, parent_root: B256) {
        if self.telemetry.pending_dedup_strategy == PendingRequestDedupStrategy::Dedup
            && self
                .pending_job_requests
                .iter()
                .any(|request| matches!(request, PendingJobRequest::Initial { root: existing_root, .. } if *existing_root == root))
        {
            return;
        }

        self.pending_job_requests
            .push_back(PendingJobRequest::new_initial(root, slot, parent_root));
    }

    async fn recover_stalled_queue(
        &mut self,
        recovery: QueueRecovery,
        stall_timeout: Duration,
    ) -> anyhow::Result<()> {
        for root in &recovery.job_roots {
            self.telemetry.inflight_roots.remove(root);
        }
        for peer_id in &recovery.peer_ids {
            self.peers_in_use.remove(peer_id);
        }

        if let Some(checkpoint) = recovery.restart_checkpoint {
            self.parse_recovery_checkpoint(
                checkpoint,
                recovery.starting_root,
                recovery.starting_slot,
                Some(stall_timeout),
                "Backfill queue stalled",
            )
            .await?;
        } else {
            warn!(
                starting_root = ?recovery.starting_root,
                starting_slot = recovery.starting_slot,
                stall_timeout_seconds = stall_timeout.as_secs_f64(),
                "Backfill queue stalled but was superseded by a newer complete queue; dropping stalled queue",
            );
        }

        Ok(())
    }

    async fn parse_recovery_checkpoint(
        &mut self,
        checkpoint: Checkpoint,
        starting_root: B256,
        starting_slot: u64,
        timeout: Option<Duration>,
        reason: &str,
    ) -> anyhow::Result<()> {
        match self.stage_recovery_checkpoint(checkpoint).await? {
            RecoveryCheckpointAction::Queued { attempts } => {
                warn!(
                    starting_root = ?starting_root,
                    starting_slot,
                    restart_root = ?checkpoint.root,
                    restart_slot = checkpoint.slot,
                    stall_timeout_seconds = ?timeout.map(|t| t.as_secs_f64()),
                    recovery_attempt = attempts,
                    max_recovery_attempts = MAX_BACKFILL_RECOVERY_ATTEMPTS,
                    "{reason}; recovery root re-queued",
                );
            }
            RecoveryCheckpointAction::AlreadyQueued => {
                debug!(
                    starting_root = ?starting_root,
                    starting_slot,
                    restart_root = ?checkpoint.root,
                    restart_slot = checkpoint.slot,
                    "{reason}; recovery root is already queued for recovery",
                );
            }
            RecoveryCheckpointAction::AlreadyQuarantined {
                attempts,
                current_head_slot,
                network_finalized_slot,
            } => {
                warn!(
                    starting_root = ?starting_root,
                    starting_slot,
                    restart_root = ?checkpoint.root,
                    restart_slot = checkpoint.slot,
                    attempts,
                    current_head_slot,
                    network_finalized_slot,
                    "{reason}; recovery root remains quarantined until the block arrives or finalized passes it",
                );
            }
            RecoveryCheckpointAction::AlreadyDropped => {
                warn!(
                    starting_root = ?starting_root,
                    starting_slot,
                    restart_root = ?checkpoint.root,
                    restart_slot = checkpoint.slot,
                    "{reason}; recovery root is already dropped",
                );
            }
            RecoveryCheckpointAction::QuarantinedByBudget {
                attempts,
                current_head_slot,
                network_finalized_slot,
            } => {
                warn!(
                    starting_root = ?starting_root,
                    starting_slot,
                    restart_root = ?checkpoint.root,
                    restart_slot = checkpoint.slot,
                    attempts,
                    max_recovery_attempts = MAX_BACKFILL_RECOVERY_ATTEMPTS,
                    current_head_slot,
                    network_finalized_slot,
                    "{reason}; quarantining recovery root until the block arrives or finalized passes it",
                );
            }
            RecoveryCheckpointAction::DroppedByBudget {
                attempts,
                current_head_slot,
                network_finalized_slot,
            } => {
                error!(
                    starting_root = ?starting_root,
                    starting_slot,
                    restart_root = ?checkpoint.root,
                    restart_slot = checkpoint.slot,
                    attempts,
                    max_recovery_attempts = MAX_BACKFILL_RECOVERY_ATTEMPTS,
                    current_head_slot,
                    network_finalized_slot,
                    "{reason}; dropping recovery root after exhausting recovery budget",
                );
            }
        }
        Ok(())
    }

    fn has_recent_near_head_gossip_bridge(
        &self,
        head: B256,
        current_head_slot: u64,
        highest_peer_head_slot: u64,
    ) -> bool {
        let gap = highest_peer_head_slot.saturating_sub(current_head_slot);
        if gap <= 1 {
            return true;
        }
        if gap > NEAR_HEAD_BRIDGE_MAX_GAP_SLOTS {
            return false;
        }

        let now = Instant::now();
        self.telemetry.recent_sync_blocks.iter().any(|block| {
            block.source == SyncBlockSource::Gossip
                && now.saturating_duration_since(block.seen_at) <= RECENT_SYNC_BLOCK_RETENTION
                && block.parent_root == head
                && block.slot > current_head_slot
                && block.slot <= highest_peer_head_slot.saturating_add(1)
        })
    }

    fn record_recent_sync_block(&mut self, parent_root: B256, slot: u64, source: SyncBlockSource) {
        self.telemetry.recent_sync_blocks.push(RecentSyncBlock {
            parent_root,
            slot,
            seen_at: Instant::now(),
            source,
        });
        self.prune_recent_sync_blocks();
    }

    fn prune_recent_sync_blocks(&mut self) {
        let now = Instant::now();
        self.telemetry.recent_sync_blocks.retain(|block| {
            now.saturating_duration_since(block.seen_at) <= RECENT_SYNC_BLOCK_RETENTION
        });
    }

    async fn handle_network_event(&mut self, event: NetworkEvent) -> anyhow::Result<()> {
        match event {
            NetworkEvent::RequestMessage {
                peer_id,
                stream_id,
                connection_id,
                message,
            } => {
                self.handle_request_network_event(peer_id, stream_id, connection_id, message)
                    .await
            }
            NetworkEvent::NetworkError { peer_id, error } => {
                trace!("Network error from peer {peer_id:?}: {error:?}");
                self.handle_failed_job_request(peer_id).await?;
                Ok(())
            }
        }
    }

    async fn handle_request_network_event(
        &mut self,
        peer_id: PeerId,
        stream_id: u64,
        connection_id: ConnectionId,
        message: LeanRequestMessage,
    ) -> anyhow::Result<()> {
        match message {
            LeanRequestMessage::BlocksByRoot(blocks_by_root_v1_request) => {
                let block_provider = {
                    let fork_choice = self.store.read().await;
                    let store = fork_choice.store.lock().await;
                    store.block_provider()
                };
                for root in blocks_by_root_v1_request.roots {
                    match block_provider.get(root) {
                        Ok(Some(block)) => {
                            if let Err(err) = self.outbound_p2p.send(LeanP2PRequest::Response {
                                peer_id,
                                stream_id,
                                connection_id,
                                message: LeanResponseMessage::BlocksByRoot(Arc::new(block)),
                            }) {
                                warn!("Failed to handle incoming lean request: {err:?}");
                            }
                        }
                        Ok(None) => {
                            debug!("Block not found for root: {root:?}");
                        }
                        Err(err) => {
                            warn!("Failed to get block for root {root:?}: {err:?}");
                        }
                    }
                }
                if let Err(err) = self.outbound_p2p.send(LeanP2PRequest::EndOfStream {
                    peer_id,
                    stream_id,
                    connection_id,
                }) {
                    warn!("Failed to send end of stream: {err:?}");
                }
            }
            _ => warn!(
                "We handle these messages elsewhere, received unexpected LeanRequestMessage: {:?}",
                message
            ),
        }
        Ok(())
    }

    async fn handle_failed_job_request(&mut self, peer_id: PeerId) -> anyhow::Result<()> {
        self.network_state.failed_response_from_peer(peer_id);
        if !self.backfill_state.has_job_for_peer(peer_id) {
            return Ok(());
        }
        self.peers_in_use.remove(&peer_id);
        self.telemetry.inflight_roots.retain(|_, inflight| {
            inflight.primary_peer != peer_id && inflight.backup_peer != Some(peer_id)
        });
        self.queue_pending_reset(peer_id);
        Ok(())
    }

    fn callback_matches_current_assignment(&self, peer_id: PeerId, root: B256) -> bool {
        if let Some(inflight) = self.telemetry.inflight_roots.get(&root) {
            return inflight.primary_peer == peer_id || inflight.backup_peer == Some(peer_id);
        }

        self.backfill_state.peer_for_job_root(root) == Some(peer_id)
    }

    async fn handle_callback_response_message(
        &mut self,
        peer_id: PeerId,
        message: Arc<LeanResponseMessage>,
    ) -> anyhow::Result<()> {
        match &*message {
            LeanResponseMessage::BlocksByRoot(signed_block) => {
                let block_root = signed_block.block.tree_hash_root();
                if !self.telemetry.inflight_roots.contains_key(&block_root)
                    && !self.backfill_state.contains_job_root(block_root)
                {
                    trace!(
                        peer_id = ?peer_id,
                        block_root = ?block_root,
                        "Ignoring stale backfill callback for completed root"
                    );
                    return Ok(());
                }
                if !self.callback_matches_current_assignment(peer_id, block_root) {
                    trace!(
                        peer_id = ?peer_id,
                        block_root = ?block_root,
                        current_job_peer_id = ?self.backfill_state.peer_for_job_root(block_root),
                        "Accepting late backfill callback for unresolved root from non-assigned peer",
                    );
                }
                if self.should_drop_callback_response(block_root) {
                    self.telemetry.backfill_telemetry.callbacks_dropped += 1;
                    warn!(
                        peer_id = ?peer_id,
                        block_root = ?block_root,
                        callback_loss_mode = ?self.telemetry.callback_loss_mode,
                        "Dropping req/resp block callback to simulate packet loss"
                    );
                    return Ok(());
                }
                self.handle_backfill_block(
                    Some(peer_id),
                    signed_block.as_ref().clone(),
                    SyncBlockSource::ReqResp,
                )
                .await?;
            }
            _ => warn!(
                "We handle these messages elsewhere, received unexpected LeanRequestMessage: {:?}",
                message
            ),
        }
        Ok(())
    }

    fn should_drop_callback_response(&mut self, root: B256) -> bool {
        match self.telemetry.callback_loss_mode {
            CallbackLossMode::None => false,
            CallbackLossMode::DropFirstPerRoot => {
                self.telemetry.dropped_callback_roots.insert(root)
            }
        }
    }
    async fn handle_syncing_process_block(
        &mut self,
        signed_block: &SignedBlock,
    ) -> anyhow::Result<()> {
        let root = signed_block.block.tree_hash_root();
        trace!(
            root = ?root,
            slot = signed_block.block.slot,
            "Received gossiped block while backfill syncing"
        );
        self.handle_backfill_block(None, signed_block.clone(), SyncBlockSource::Gossip)
            .await
    }

    async fn try_advance_job_with_cached_block(&mut self, root: B256) -> anyhow::Result<bool> {
        if self.is_suppressed_backfill_root(&root) {
            trace!(
                root = ?root,
                "Skipping cached pending block because the root is suppressed pending fresh arrival"
            );
            return Ok(false);
        }

        let pending_block = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            store.pending_blocks_provider().get(root)?
        };

        if let Some(block) = pending_block {
            trace!(
                root = ?root,
                "Using cached pending block to advance backfill queue"
            );
            self.handle_backfill_block(None, block, SyncBlockSource::ReqResp)
                .await?;
            return Ok(true);
        }

        Ok(false)
    }

    async fn handle_backfill_block(
        &mut self,
        source_peer_id: Option<PeerId>,
        signed_block: SignedBlock,
        source: SyncBlockSource,
    ) -> anyhow::Result<()> {
        let last_root = signed_block.block.tree_hash_root();
        let parent_root = signed_block.block.parent_root;
        let slot = signed_block.block.slot;
        self.telemetry.inflight_roots.remove(&last_root);
        let mut request_latency_ms: Option<f64> = None;
        if source == SyncBlockSource::ReqResp {
            self.telemetry.backfill_telemetry.callbacks_processed += 1;
            if let Some(latency) = self.backfill_state.request_latency_for_root(last_root) {
                self.telemetry.backfill_telemetry.callback_latency_ms_total += latency.as_millis();
                self.telemetry.backfill_telemetry.callback_latency_samples += 1;
                request_latency_ms = Some(latency.as_secs_f64() * 1_000.0);
            }
        }
        let job_peer_id = self.backfill_state.peer_for_job_root(last_root);
        let (head, pending_blocks_provider) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (
                store.head_provider().get()?,
                store.pending_blocks_provider(),
            )
        };
        pending_blocks_provider.insert(last_root, signed_block)?;
        self.clear_backfill_arrival_state(last_root);
        self.record_recent_sync_block(parent_root, slot, source);

        if let Some(job_peer_id) = job_peer_id {
            self.peers_in_use.remove(&job_peer_id);
        }

        if let Some(peer_id) = source_peer_id {
            self.network_state.successful_response_from_peer(peer_id);
            if let Some(latency_ms) = request_latency_ms {
                self.telemetry
                    .peer_avg_latency_ms
                    .entry(peer_id)
                    .and_modify(|avg_ms| *avg_ms = (*avg_ms * 0.8) + (latency_ms * 0.2))
                    .or_insert(latency_ms);
            }
            self.peers_in_use.remove(&peer_id);
        }

        let parent_root_is_start_of_any_queue =
            self.backfill_state.is_root_start_of_any_queue(&parent_root);
        if parent_root_is_start_of_any_queue
            && let Some(absorption) =
                self.backfill_state
                    .absorb_queue_frontier(last_root, slot, parent_root)
        {
            info!(
                completed_root = ?last_root,
                completed_slot = slot,
                absorbed_queue_root = ?absorption.absorbed_starting_root,
                absorbed_queue_slot = absorption.absorbed_starting_slot,
                merged_job_count = absorption.merged_job_count,
                "Absorbed older backfill queue frontier into newer queue",
            );
            self.queue_pending_job_requests().await?;
            return Ok(());
        }

        let parent_resolution = if parent_root_is_start_of_any_queue {
            BackfillParentResolution::Complete {
                completion_root: parent_root,
            }
        } else {
            self.resolve_backfill_parent_resolution(head, parent_root, slot)
                .await?
        };

        match parent_resolution {
            BackfillParentResolution::Complete { completion_root } => {
                trace!(
                    root = ?last_root,
                    parent_root = ?parent_root,
                    completion_root = ?completion_root,
                    "Marking backfill queue as complete"
                );
                self.backfill_state
                    .mark_job_queue_as_complete_at(last_root, Some(completion_root));
            }
            BackfillParentResolution::NeedsRequest {
                request_slot,
                missing_root,
            } => {
                if self.backfill_state.contains_job_root(last_root) {
                    self.queue_pending_initial(last_root, request_slot, missing_root);
                    self.queue_pending_job_requests().await?;
                }
            }
        }

        Ok(())
    }

    async fn queue_pending_job_requests(&mut self) -> anyhow::Result<()> {
        while let Some(pending_job_request) = self.pending_job_requests.pop_front() {
            let avoid_peer_id = match &pending_job_request {
                PendingJobRequest::Reset { peer_id } => Some(*peer_id),
                PendingJobRequest::Initial { .. } => None,
            };
            let preferred_checkpoint = match &pending_job_request {
                PendingJobRequest::Reset { peer_id } => {
                    self.backfill_state.checkpoint_for_peer(*peer_id)
                }
                PendingJobRequest::Initial { .. } => None,
            };
            let preferred_slot = match &pending_job_request {
                PendingJobRequest::Reset { peer_id } => {
                    self.backfill_state.expected_slot_for_peer(*peer_id)
                }
                PendingJobRequest::Initial { slot, .. } => Some(slot.saturating_sub(1)),
            };
            let non_queued_peer_id = match if let Some(checkpoint) = preferred_checkpoint {
                self.assignable_peer_id_for_checkpoint(checkpoint, avoid_peer_id)
                    .await
            } else if let Some(slot) = preferred_slot {
                self.assignable_peer_id_for_slot(slot, avoid_peer_id).await
            } else {
                self.assignable_peer_id(avoid_peer_id).await
            } {
                Some(id) => id,
                None => {
                    info!(
                        sync_status = ?self.sync_status,
                        request = ?pending_job_request,
                        queue_count = self.backfill_state.jobs.len(),
                        pending_requests_remaining = self.pending_job_requests.len(),
                        connected_peer_count = self.network_state.connected_peer_count(),
                        peers_in_use = self.peers_in_use.len(),
                        "No assignable peer available for pending backfill job request",
                    );
                    self.pending_job_requests.push_back(pending_job_request);
                    return Ok(());
                }
            };
            match pending_job_request {
                PendingJobRequest::Reset { peer_id } => {
                    if self
                        .backfill_state
                        .reset_job_with_new_peer_id(peer_id, non_queued_peer_id)
                        .is_none()
                    {
                        warn!(
                            "Failed to reset pending job request for peer {peer_id:?} - no matching job found.",
                        );
                        continue;
                    }
                }
                PendingJobRequest::Initial {
                    root,
                    slot,
                    parent_root,
                } => {
                    if self
                        .backfill_state
                        .replace_job_with_next_job(
                            root,
                            slot,
                            JobRequest::new(non_queued_peer_id, parent_root),
                        )
                        .is_none()
                    {
                        warn!(
                            root = ?root,
                            parent_root = ?parent_root,
                            slot,
                            "Failed to attach initial pending job request to existing queue; rebuilding as fresh queue",
                        );
                        let new_queue_added = self.backfill_state.add_new_job_queue(
                            Checkpoint {
                                root: parent_root,
                                slot: slot.saturating_sub(1),
                            },
                            JobRequest::new(non_queued_peer_id, parent_root),
                            true,
                        );
                        if !new_queue_added {
                            warn!(
                                root = ?root,
                                parent_root = ?parent_root,
                                slot,
                                "Failed to rebuild initial pending job request as fresh queue",
                            );
                            continue;
                        }
                    }
                }
            }
            self.peers_in_use.insert(non_queued_peer_id);
        }
        Ok(())
    }

    async fn prune_old_state(&self, tick_count: u64) -> anyhow::Result<()> {
        let (head, block_provider, slot_index_provider, state_provider) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;

            if get_current_slot().is_multiple_of(15)
                && let Err(err) = store.report_storage_metrics(0)
            {
                warn!("Failed to report storage metrics: {err:?}");
            }
            (
                store.head_provider().get()?,
                store.block_provider(),
                store.slot_index_provider(),
                store.state_provider(),
            )
        };

        let head_slot = block_provider
            .get(head)?
            .ok_or_else(|| anyhow!("State not found for head: {head}"))?
            .block
            .slot;

        if head_slot > STATE_RETENTION_SLOTS {
            let prune_target_slot = head_slot - STATE_RETENTION_SLOTS;
            let block_root = slot_index_provider
                .get(prune_target_slot)?
                .ok_or_else(|| anyhow!("Block root not found for slot: {prune_target_slot}"))?;

            info!(
                slot = get_current_slot(),
                tick = tick_count,
                prune_slot = prune_target_slot,
                prune_block_root = ?block_root,
                "Pruning old lean state"
            );

            if let Err(err) = state_provider.remove(block_root) {
                warn!("Failed to prune old lean state: {err:?}");
            }
        }
        Ok(())
    }

    async fn handle_produce_block(
        &mut self,
        slot: u64,
        response: oneshot::Sender<ServiceResponse<BlockWithSignatures>>,
    ) -> anyhow::Result<()> {
        let lock_acq_start = Instant::now();
        let mut store_guard = self.store.write().await;
        let lock_acq_elapsed = lock_acq_start.elapsed();
        let produce_start = Instant::now();
        let block_with_signatures = match store_guard
            .produce_block_with_signatures(slot, slot % lean_network_spec().num_validators)
            .await
        {
            Ok(block) => block,
            Err(err) => {
                info!(
                    slot,
                    lock_wait_ms = lock_acq_elapsed.as_millis() as u64,
                    produce_ms = produce_start.elapsed().as_millis() as u64,
                    "DIAG handle_produce_block FAILED",
                );
                warn!("Failed to produce block for slot {slot}: {err}");
                if let Err(err) = response.send(ServiceResponse::Err(err)) {
                    warn!("Failed to send error response for ProduceBlock: {err:?}");
                }
                return Ok(());
            }
        };
        info!(
            slot,
            lock_wait_ms = lock_acq_elapsed.as_millis() as u64,
            produce_ms = produce_start.elapsed().as_millis() as u64,
            "DIAG handle_produce_block ok",
        );

        response
            .send(ServiceResponse::Ok(block_with_signatures))
            .map_err(|err| {
                anyhow!(
                    "Failed to send produced block: {}...",
                    format!("{err:?}").chars().take(100).collect::<String>()
                )
            })?;

        Ok(())
    }

    async fn handle_build_attestation_data(
        &mut self,
        slot: u64,
        response: oneshot::Sender<ServiceResponse<AttestationData>>,
    ) -> anyhow::Result<()> {
        let attestation_data = match self.store.read().await.produce_attestation_data(slot).await {
            Ok(data) => data,
            Err(err) => {
                warn!("Failed to build attestation data for slot {slot}: {err}");
                if let Err(err) = response.send(ServiceResponse::Err(err)) {
                    warn!("Failed to send error response for BuildAttestationData: {err:?}");
                }
                return Ok(());
            }
        };

        response
            .send(ServiceResponse::Ok(attestation_data))
            .map_err(|err| anyhow!("Failed to send built attestation data: {err:?}"))?;

        Ok(())
    }
    async fn handle_process_block(&mut self, signed_block: &SignedBlock) -> anyhow::Result<()> {
        let parent_root = signed_block.block.parent_root;
        let has_parent_state = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            store.state_provider().get(parent_root)?.is_some()
        };
        if !has_parent_state {
            warn!(
                root = ?signed_block.block.tree_hash_root(),
                parent_root = ?parent_root,
                "Missing parent state while processing synced block; routing block to backfill path"
            );
            return self.handle_syncing_process_block(signed_block).await;
        }

        self.store
            .write()
            .await
            .on_block(signed_block, true)
            .await?;

        Ok(())
    }

    async fn handle_process_attestation(
        &mut self,
        signed_attestation: SignedAttestation,
    ) -> anyhow::Result<()> {
        self.store
            .write()
            .await
            .on_gossip_attestation(signed_attestation, self.is_aggregator)
            .await?;

        Ok(())
    }

    fn push_callback_receiver(&mut self, rx: tokio::sync::mpsc::Receiver<ResponseCallback>) {
        let future: CallbackFuture = Box::pin(async move {
            let mut rx = rx;
            let message = rx.recv().await;
            (message, rx)
        });

        self.pending_callbacks.push(future);
    }
}

fn pending_block_slot(block: &SignedBlock) -> u64 {
    block.block.slot
}

fn pending_block_parent_root(block: &SignedBlock) -> B256 {
    block.block.parent_root
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    use alloy_primitives::B256;
    use libp2p_identity::PeerId;
    use ream_consensus_lean::{
        block::{BlockSignatures, BlockWithSignatures, SignedBlock},
        checkpoint::Checkpoint,
    };
    use ream_fork_choice_lean::store::Store;
    use ream_peer::{ConnectionState, Direction};
    use ream_post_quantum_crypto::leansig::signature::Signature;
    use ream_req_resp::lean::messages::LeanResponseMessage;
    use ream_storage::tables::{field::REDBField, table::REDBTable};
    use ream_sync::rwlock::Writer;
    use ream_test_utils::store::sample_store;
    use tokio::sync::{mpsc, oneshot};
    use tree_hash::TreeHash;

    use super::{InflightRootRequest, LeanChainService, SyncBlockSource};
    use crate::{
        messages::ServiceResponse,
        p2p_request::LeanP2PRequest,
        sync::{
            SyncStatus,
            forward_background_syncer::ForwardSyncResults,
            job::{pending::PendingJobRequest, queue::JobQueue, request::JobRequest},
        },
    };

    async fn advance_store_to_slot(store: &mut Store, target_slot: u64, validator_count: u64) {
        for slot in 1..=target_slot {
            let proposer_index = slot % validator_count;
            let block = store
                .produce_block_with_signatures(slot, proposer_index)
                .await
                .unwrap();
            let signed_block = SignedBlock {
                block: block.block,
                signature: BlockSignatures {
                    attestation_signatures: block.signatures,
                    proposer_signature: Signature::blank(),
                },
            };
            store.on_block(&signed_block, false).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_assignable_peer_id_falls_back_to_connected_peer_when_all_are_in_use() {
        let store = sample_store(10).await;
        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.peers_in_use.insert(peer_id);

        assert_eq!(service.assignable_peer_id(None).await, Some(peer_id));
        assert_eq!(service.assignable_peer_id(Some(peer_id)).await, None);
    }

    #[tokio::test]
    async fn test_assignable_peer_id_for_checkpoint_prefers_matching_head_peer() {
        let store = sample_store(10).await;
        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

        let service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let target_checkpoint = Checkpoint {
            root: B256::repeat_byte(0x44),
            slot: 7,
        };
        let matching_peer = PeerId::random();
        let lagging_peer = PeerId::random();
        for peer_id in [matching_peer, lagging_peer] {
            service.network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }
        service.network_state.update_peer_checkpoints(
            matching_peer,
            target_checkpoint,
            Checkpoint {
                root: B256::repeat_byte(0x01),
                slot: 2,
            },
        );
        service.network_state.update_peer_checkpoints(
            lagging_peer,
            Checkpoint {
                root: B256::repeat_byte(0x45),
                slot: 5,
            },
            Checkpoint {
                root: B256::repeat_byte(0x02),
                slot: 2,
            },
        );

        assert_eq!(
            service
                .assignable_peer_id_for_checkpoint(target_checkpoint, None)
                .await,
            Some(matching_peer)
        );
    }

    #[tokio::test]
    async fn test_backfill_queue_progress_summary_reports_active_and_complete_queues() {
        let store = sample_store(10).await;
        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let root_active_start = B256::repeat_byte(0x11);
        let root_active_progress = B256::repeat_byte(0x22);
        let root_complete = B256::repeat_byte(0x33);
        let parent_root = B256::repeat_byte(0x44);
        let peer_id = PeerId::random();

        let mut waiting_on_start = JobQueue::new(root_active_start, 100, 100);
        waiting_on_start.add_job(JobRequest::new(peer_id, root_active_start));

        let mut in_progress = JobQueue::new(root_active_progress, 80, 80);
        in_progress.add_job(JobRequest::new(peer_id, parent_root));

        let mut complete = JobQueue::new(root_complete, 60, 57);
        complete.is_complete = true;

        service.backfill_state.jobs = vec![waiting_on_start, in_progress, complete];

        assert_eq!(
            service.backfill_queue_progress_summary(),
            "active(start=100, fetched_through=none, waiting_on=100, jobs=1); \
active(start=80, fetched_through=80, waiting_on=79, jobs=1); \
complete(start=60, fetched_through=57, jobs=0)"
        );
    }

    #[tokio::test]
    async fn test_handle_produce_block_stale_slot_responds_with_err() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        // Try to produce a block for slot 2 (behind head slot 3)
        let (tx, rx) = oneshot::channel::<ServiceResponse<BlockWithSignatures>>();
        let _ = service.handle_produce_block(2, tx).await;

        assert!(
            rx.await.is_ok(),
            "Channel was dropped instead of sending a response"
        );
    }

    #[tokio::test]
    async fn test_update_sync_status_stays_synced_when_only_behind_peer_head() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: B256::repeat_byte(0x11),
                slot: 7,
            },
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        assert_eq!(
            service.update_sync_status().await.unwrap(),
            SyncStatus::Synced
        );
    }

    #[tokio::test]
    async fn test_should_run_backfill_sync_waits_for_persistent_peer_gap_when_synced() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: B256::repeat_byte(0x11),
                slot: 7,
            },
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        assert!(!service.should_run_backfill_sync().await);

        tokio::time::sleep(
            super::SYNCED_BACKFILL_GAP_PERSISTENCE_THRESHOLD + Duration::from_millis(50),
        )
        .await;

        assert!(service.should_run_backfill_sync().await);
    }

    #[tokio::test]
    async fn test_current_peer_gap_slots_uses_small_devnet_fallback_when_two_peers_split_head() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let shared_finalized = Checkpoint {
            root: B256::repeat_byte(0x22),
            slot: 2,
        };

        for peer_id in [peer_a, peer_b] {
            service.network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }
        service.network_state.update_peer_checkpoints(
            peer_a,
            Checkpoint {
                root: B256::repeat_byte(0x11),
                slot: 5,
            },
            shared_finalized,
        );
        service.network_state.update_peer_checkpoints(
            peer_b,
            Checkpoint {
                root: B256::repeat_byte(0x12),
                slot: 7,
            },
            shared_finalized,
        );

        assert_eq!(service.current_peer_gap_slots().await, 4);
    }

    #[tokio::test]
    async fn test_step_backfill_sync_starts_queue_while_synced_when_only_behind_peer_head() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        let head_checkpoint = Checkpoint {
            root: B256::repeat_byte(0x11),
            slot: 7,
        };
        service.network_state.update_peer_checkpoints(
            peer_id,
            head_checkpoint,
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        service.step_backfill_sync().await.unwrap();

        assert_eq!(service.sync_status, SyncStatus::Synced);
        assert_eq!(service.backfill_state.jobs.len(), 1);
        assert_eq!(
            service.backfill_state.jobs[0].starting_root,
            head_checkpoint.root
        );
        assert_eq!(
            service.backfill_state.jobs[0].starting_slot,
            head_checkpoint.slot
        );
    }

    #[tokio::test]
    async fn test_step_backfill_sync_uses_small_devnet_fallback_when_two_peers_split_head() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let shared_finalized = Checkpoint {
            root: B256::repeat_byte(0x22),
            slot: 2,
        };
        let fallback_head_checkpoint = Checkpoint {
            root: B256::repeat_byte(0x12),
            slot: 7,
        };

        for peer_id in [peer_a, peer_b] {
            service.network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }
        service.network_state.update_peer_checkpoints(
            peer_a,
            Checkpoint {
                root: B256::repeat_byte(0x11),
                slot: 5,
            },
            shared_finalized,
        );
        service.network_state.update_peer_checkpoints(
            peer_b,
            fallback_head_checkpoint,
            shared_finalized,
        );

        service.step_backfill_sync().await.unwrap();

        assert_eq!(service.sync_status, SyncStatus::Synced);
        assert_eq!(service.backfill_state.jobs.len(), 1);
        assert_eq!(
            service.backfill_state.jobs[0].starting_root,
            fallback_head_checkpoint.root
        );
        assert_eq!(
            service.backfill_state.jobs[0].starting_slot,
            fallback_head_checkpoint.slot
        );
    }

    #[tokio::test]
    async fn test_queue_pending_job_requests_initial_prefers_peer_at_or_above_parent_slot() {
        let store = sample_store(10).await;
        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let root = B256::repeat_byte(0x11);
        let parent_root = B256::repeat_byte(0x22);
        let old_peer = PeerId::random();
        let eligible_peer = PeerId::random();
        let lagging_peer = PeerId::random();

        service.backfill_state.add_new_job_queue(
            Checkpoint { root, slot: 7 },
            JobRequest::new(old_peer, root),
            true,
        );
        service
            .pending_job_requests
            .push_back(PendingJobRequest::new_initial(root, 7, parent_root));

        for peer_id in [eligible_peer, lagging_peer] {
            service.network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }
        service.network_state.update_peer_checkpoints(
            eligible_peer,
            Checkpoint {
                root: B256::repeat_byte(0x33),
                slot: 8,
            },
            Checkpoint {
                root: B256::repeat_byte(0x34),
                slot: 2,
            },
        );
        service.network_state.update_peer_checkpoints(
            lagging_peer,
            Checkpoint {
                root: B256::repeat_byte(0x35),
                slot: 5,
            },
            Checkpoint {
                root: B256::repeat_byte(0x36),
                slot: 2,
            },
        );

        service.queue_pending_job_requests().await.unwrap();

        assert_eq!(
            service.backfill_state.peer_for_job_root(parent_root),
            Some(eligible_peer)
        );
    }

    #[tokio::test]
    async fn test_step_backfill_sync_skips_peer_checkpoint_already_behind_local_head() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 8, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: B256::repeat_byte(0x11),
                slot: 6,
            },
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        service.step_backfill_sync().await.unwrap();

        assert!(service.backfill_state.jobs.is_empty());
        assert!(service.checkpoints_to_queue.is_empty());
    }

    #[tokio::test]
    async fn test_step_backfill_sync_skips_peer_checkpoint_already_in_canonical_store() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let block = store.produce_block_with_signatures(4, 4).await.unwrap();
        let signed_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::blank(),
            },
        };
        let checkpoint_root = signed_block.block.tree_hash_root();

        // Insert a newer canonical block without updating the head provider.
        store.on_block(&signed_block, false).await.unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Syncing;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: checkpoint_root,
                slot: 4,
            },
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        service.step_backfill_sync().await.unwrap();

        assert!(service.backfill_state.jobs.is_empty());
        assert!(service.checkpoints_to_queue.is_empty());
    }

    #[tokio::test]
    async fn test_step_backfill_sync_skips_peer_checkpoint_already_pending_while_other_backfill_work_exists()
     {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let block = store.produce_block_with_signatures(4, 4).await.unwrap();
        let signed_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::blank(),
            },
        };
        let pending_root = signed_block.block.tree_hash_root();
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_root, signed_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Syncing;

        let old_queue_root = B256::repeat_byte(0x99);
        let old_peer = PeerId::random();
        service.backfill_state.add_new_job_queue(
            Checkpoint {
                root: old_queue_root,
                slot: 1,
            },
            JobRequest::new(old_peer, old_queue_root),
            true,
        );

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: pending_root,
                slot: 4,
            },
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        service.step_backfill_sync().await.unwrap();

        assert_eq!(service.backfill_state.jobs.len(), 1);
        assert_eq!(service.backfill_state.jobs[0].starting_root, old_queue_root);
        assert!(service.checkpoints_to_queue.is_empty());
    }

    #[tokio::test]
    async fn test_step_backfill_sync_drops_stale_staged_peer_checkpoint_without_current_common_checkpoint()
     {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;
        service.checkpoints_to_queue.push((
            Checkpoint {
                root: B256::repeat_byte(0x77),
                slot: 7,
            },
            false,
        ));

        service.step_backfill_sync().await.unwrap();

        assert!(service.backfill_state.jobs.is_empty());
        assert!(service.checkpoints_to_queue.is_empty());
    }

    #[tokio::test]
    async fn test_step_backfill_sync_replaces_stale_staged_peer_checkpoint_with_current_common_checkpoint()
     {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Syncing;
        service.checkpoints_to_queue.push((
            Checkpoint {
                root: B256::repeat_byte(0x66),
                slot: 7,
            },
            false,
        ));

        let peer_id = PeerId::random();
        let current_common_checkpoint = Checkpoint {
            root: B256::repeat_byte(0x55),
            slot: 6,
        };
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            current_common_checkpoint,
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        service.step_backfill_sync().await.unwrap();

        assert_eq!(service.backfill_state.jobs.len(), 1);
        assert_eq!(
            service.backfill_state.jobs[0].starting_root,
            current_common_checkpoint.root
        );
        assert_eq!(
            service.backfill_state.jobs[0].starting_slot,
            current_common_checkpoint.slot
        );
        assert!(service.checkpoints_to_queue.is_empty());
    }

    #[tokio::test]
    async fn test_step_backfill_sync_skips_peer_checkpoint_with_quarantined_root() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        let quarantined_root = B256::repeat_byte(0x33);
        service.quarantined_backfill_roots.insert(
            quarantined_root,
            super::QuarantinedBackfillRoot {
                slot: 7,
                attempts: super::MAX_BACKFILL_RECOVERY_ATTEMPTS + 1,
            },
        );

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: quarantined_root,
                slot: 7,
            },
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: 2,
            },
        );

        service.step_backfill_sync().await.unwrap();

        assert!(service.backfill_state.jobs.is_empty());
        assert!(service.checkpoints_to_queue.is_empty());
    }

    #[tokio::test]
    async fn test_stage_recovery_checkpoint_quarantines_root_before_finalized() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        let checkpoint = Checkpoint {
            root: B256::repeat_byte(0xbb),
            slot: 5,
        };

        for attempt in 1..=super::MAX_BACKFILL_RECOVERY_ATTEMPTS {
            match service.stage_recovery_checkpoint(checkpoint).await.unwrap() {
                super::RecoveryCheckpointAction::Queued { attempts } => {
                    assert_eq!(attempts, attempt);
                }
                other => panic!("expected queued recovery action, got {other:?}"),
            }
            service.checkpoints_to_queue.clear();
        }

        match service.stage_recovery_checkpoint(checkpoint).await.unwrap() {
            super::RecoveryCheckpointAction::QuarantinedByBudget {
                attempts,
                current_head_slot,
                network_finalized_slot,
            } => {
                assert_eq!(attempts, super::MAX_BACKFILL_RECOVERY_ATTEMPTS + 1);
                assert_eq!(current_head_slot, 3);
                assert_eq!(network_finalized_slot, 3);
            }
            other => panic!("expected quarantined-by-budget action, got {other:?}"),
        }

        assert!(service.checkpoints_to_queue.is_empty());
        assert!(
            service
                .quarantined_backfill_roots
                .contains_key(&checkpoint.root)
        );
        assert!(!service.dropped_backfill_roots.contains(&checkpoint.root));
        assert!(
            !service
                .backfill_recovery_attempts
                .contains_key(&checkpoint.root)
        );
        assert!(matches!(
            service.stage_recovery_checkpoint(checkpoint).await.unwrap(),
            super::RecoveryCheckpointAction::AlreadyQuarantined { .. }
        ));
    }

    #[tokio::test]
    async fn test_stage_recovery_checkpoint_drops_root_after_retry_budget_once_finalized() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        let checkpoint = Checkpoint {
            root: B256::repeat_byte(0xaa),
            slot: 1,
        };

        for attempt in 1..=super::MAX_BACKFILL_RECOVERY_ATTEMPTS {
            match service.stage_recovery_checkpoint(checkpoint).await.unwrap() {
                super::RecoveryCheckpointAction::Queued { attempts } => {
                    assert_eq!(attempts, attempt);
                }
                other => panic!("expected queued recovery action, got {other:?}"),
            }
            service.checkpoints_to_queue.clear();
        }

        match service.stage_recovery_checkpoint(checkpoint).await.unwrap() {
            super::RecoveryCheckpointAction::DroppedByBudget {
                attempts,
                current_head_slot,
                network_finalized_slot,
            } => {
                assert_eq!(attempts, super::MAX_BACKFILL_RECOVERY_ATTEMPTS + 1);
                assert_eq!(current_head_slot, 3);
                assert_eq!(network_finalized_slot, 3);
            }
            other => panic!("expected dropped-by-budget action, got {other:?}"),
        }

        assert!(service.checkpoints_to_queue.is_empty());
        assert!(service.dropped_backfill_roots.contains(&checkpoint.root));
        assert!(
            !service
                .backfill_recovery_attempts
                .contains_key(&checkpoint.root)
        );
        assert!(matches!(
            service.stage_recovery_checkpoint(checkpoint).await.unwrap(),
            super::RecoveryCheckpointAction::AlreadyDropped
        ));
    }

    #[tokio::test]
    async fn test_refresh_quarantined_backfill_roots_drops_once_finalized_passes() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: B256::repeat_byte(0x44),
                slot: 8,
            },
            Checkpoint {
                root: B256::repeat_byte(0x55),
                slot: 6,
            },
        );

        let root = B256::repeat_byte(0xcc);
        service.backfill_recovery_attempts.insert(root, 9);
        service.quarantined_backfill_roots.insert(
            root,
            super::QuarantinedBackfillRoot {
                slot: 5,
                attempts: 9,
            },
        );

        service.refresh_quarantined_backfill_roots().await.unwrap();

        assert!(!service.quarantined_backfill_roots.contains_key(&root));
        assert!(service.dropped_backfill_roots.contains(&root));
        assert!(!service.backfill_recovery_attempts.contains_key(&root));
    }

    #[tokio::test]
    async fn test_update_sync_status_transitions_to_syncing_when_behind_peer_finalized() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        service.network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: B256::repeat_byte(0x33),
                slot: 7,
            },
            Checkpoint {
                root: B256::repeat_byte(0x44),
                slot: 4,
            },
        );

        assert!(matches!(
            service.update_sync_status().await.unwrap(),
            SyncStatus::Syncing
        ));
    }

    #[tokio::test]
    async fn test_update_sync_status_stays_syncing_while_orphaned_pending_blocks_exist() {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let mut pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        let pending_root = {
            pending_block.block.parent_root = B256::repeat_byte(0x88);
            pending_block.block.tree_hash_root()
        };
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_root, pending_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Syncing;

        assert!(service.has_orphaned_pending_blocks().await);
        assert!(matches!(
            service.update_sync_status().await.unwrap(),
            SyncStatus::Syncing
        ));
        assert!(service.should_run_backfill_sync().await);
    }

    #[tokio::test]
    async fn test_update_sync_status_stays_synced_with_only_orphaned_pending_blocks() {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let mut pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        let pending_root = {
            pending_block.block.parent_root = B256::repeat_byte(0x99);
            pending_block.block.tree_hash_root()
        };
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_root, pending_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        assert!(service.has_orphaned_pending_blocks().await);
        assert_eq!(
            service.update_sync_status().await.unwrap(),
            SyncStatus::Synced
        );
        assert!(service.should_run_backfill_sync().await);
    }

    #[tokio::test]
    async fn test_update_sync_status_stays_synced_with_only_inflight_backfill_requests() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;
        service.telemetry.inflight_roots.insert(
            B256::repeat_byte(0xaa),
            InflightRootRequest {
                primary_peer: PeerId::random(),
                backup_peer: None,
                requested_at: Instant::now(),
                backup_sent: false,
            },
        );

        assert_eq!(
            service.update_sync_status().await.unwrap(),
            SyncStatus::Synced
        );
        assert!(service.should_run_backfill_sync().await);
    }

    #[tokio::test]
    async fn test_handle_forward_sync_root_mismatch_requeues_before_network_finalized() {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        let actual_root = pending_block.block.tree_hash_root();
        let bad_root = B256::repeat_byte(0xab);
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(bad_root, pending_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        let mut previous_queue = JobQueue::new(bad_root, 1, 1);
        previous_queue.is_complete = true;
        service.sync_status = SyncStatus::Syncing;
        service.backfill_state.jobs.push(previous_queue.clone());

        service
            .handle_forward_sync_result(ForwardSyncResults::RootMismatch {
                previous_queue,
                checkpoint_for_new_queue: Some(Checkpoint {
                    root: bad_root,
                    slot: 1,
                }),
                bad_root,
                bad_slot: 1,
                actual_root,
                network_finalized_slot: 0,
            })
            .await
            .unwrap();

        assert_eq!(service.sync_status, SyncStatus::Syncing);
        assert!(service.backfill_state.jobs.is_empty());
        assert_eq!(
            service.checkpoints_to_queue,
            vec![(
                Checkpoint {
                    root: bad_root,
                    slot: 1
                },
                true
            )]
        );
        let pending_present = {
            let fork_choice = service.store.read().await;
            let store = fork_choice.store.lock().await;
            store
                .pending_blocks_provider()
                .get(bad_root)
                .unwrap()
                .is_some()
        };
        assert!(!pending_present);
    }

    #[tokio::test]
    async fn test_handle_forward_sync_root_mismatch_drops_after_network_finalized() {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        let actual_root = pending_block.block.tree_hash_root();
        let bad_root = B256::repeat_byte(0xcd);
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(bad_root, pending_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        let mut previous_queue = JobQueue::new(bad_root, 1, 1);
        previous_queue.is_complete = true;
        service.sync_status = SyncStatus::Syncing;
        service.backfill_state.jobs.push(previous_queue.clone());

        service
            .handle_forward_sync_result(ForwardSyncResults::RootMismatch {
                previous_queue,
                checkpoint_for_new_queue: None,
                bad_root,
                bad_slot: 1,
                actual_root,
                network_finalized_slot: 1,
            })
            .await
            .unwrap();

        assert_eq!(service.sync_status, SyncStatus::Syncing);
        assert!(service.backfill_state.jobs.is_empty());
        assert!(service.checkpoints_to_queue.is_empty());
        let pending_present = {
            let fork_choice = service.store.read().await;
            let store = fork_choice.store.lock().await;
            store
                .pending_blocks_provider()
                .get(bad_root)
                .unwrap()
                .is_some()
        };
        assert!(!pending_present);
        assert!(service.dropped_backfill_roots.contains(&bad_root));
    }

    #[tokio::test]
    async fn test_try_advance_job_with_cached_block_skips_suppressed_root() {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        let pending_root = pending_block.block.tree_hash_root();
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_root, pending_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.dropped_backfill_roots.insert(pending_root);

        let advanced = service
            .try_advance_job_with_cached_block(pending_root)
            .await
            .unwrap();

        assert!(!advanced);
        let pending_present = {
            let fork_choice = service.store.read().await;
            let store = fork_choice.store.lock().await;
            store
                .pending_blocks_provider()
                .get(pending_root)
                .unwrap()
                .is_some()
        };
        assert!(pending_present);
    }

    #[tokio::test]
    async fn test_handle_backfill_block_does_not_complete_queue_through_suppressed_parent_root() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 2, 10).await;
        let suppressed_parent_root = {
            let db = store.store.lock().await;
            db.slot_index_provider().get(1).unwrap().unwrap()
        };
        let block = store.produce_block_with_signatures(3, 3).await.unwrap();
        let mut signed_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        {
            signed_block.block.parent_root = suppressed_parent_root;
        }
        let last_root = signed_block.block.tree_hash_root();
        assert_eq!(signed_block.block.parent_root, suppressed_parent_root);

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service
            .dropped_backfill_roots
            .insert(suppressed_parent_root);

        let mut queue = JobQueue::new(last_root, 1, 1);
        queue.add_job(JobRequest::new(PeerId::random(), last_root));
        service.backfill_state.jobs.push(queue);

        service
            .handle_backfill_block(None, signed_block, SyncBlockSource::Gossip)
            .await
            .unwrap();

        assert_eq!(service.backfill_state.jobs.len(), 1);
        assert!(!service.backfill_state.jobs[0].is_complete);
        assert!(matches!(
            service.pending_job_requests.front(),
            Some(PendingJobRequest::Initial {
                root,
                parent_root,
                ..
            }) if *root == last_root && *parent_root == suppressed_parent_root
        ));
    }

    #[tokio::test]
    async fn test_handle_backfill_block_completes_through_pending_chain_to_canonical_root() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 2, 10).await;
        let canonical_completion_root = {
            let db = store.store.lock().await;
            db.slot_index_provider().get(1).unwrap().unwrap()
        };

        let block = store.produce_block_with_signatures(3, 3).await.unwrap();
        let mut pending_parent = SignedBlock {
            block: block.block.clone(),
            signature: BlockSignatures {
                attestation_signatures: block.signatures.clone(),
                proposer_signature: Signature::mock(),
            },
        };
        {
            pending_parent.block.parent_root = canonical_completion_root;
        }
        let pending_parent_root = pending_parent.block.tree_hash_root();
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_parent_root, pending_parent)
            .unwrap();

        let mut signed_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        {
            signed_block.block.slot = 4;
            signed_block.block.parent_root = pending_parent_root;
        }
        let last_root = signed_block.block.tree_hash_root();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let mut queue = JobQueue::new(last_root, 4, 4);
        queue.add_job(JobRequest::new(PeerId::random(), last_root));
        service.backfill_state.jobs.push(queue);

        service
            .handle_backfill_block(None, signed_block, SyncBlockSource::Gossip)
            .await
            .unwrap();

        assert_eq!(service.backfill_state.jobs.len(), 1);
        assert!(service.backfill_state.jobs[0].is_complete);
        assert_eq!(
            service.backfill_state.jobs[0].completion_root,
            Some(canonical_completion_root)
        );
        assert!(service.pending_job_requests.is_empty());
    }

    #[tokio::test]
    async fn test_handle_backfill_block_requeues_first_missing_root_beyond_pending_chain() {
        let mut store = sample_store(10).await;
        let missing_root = B256::repeat_byte(0x66);

        let block = store.produce_block_with_signatures(3, 3).await.unwrap();
        let mut pending_parent = SignedBlock {
            block: block.block.clone(),
            signature: BlockSignatures {
                attestation_signatures: block.signatures.clone(),
                proposer_signature: Signature::mock(),
            },
        };
        {
            pending_parent.block.parent_root = missing_root;
        }
        let pending_parent_root = pending_parent.block.tree_hash_root();
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_parent_root, pending_parent)
            .unwrap();

        let mut signed_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        {
            signed_block.block.slot = 4;
            signed_block.block.parent_root = pending_parent_root;
        }
        let last_root = signed_block.block.tree_hash_root();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let mut queue = JobQueue::new(last_root, 4, 4);
        queue.add_job(JobRequest::new(PeerId::random(), last_root));
        service.backfill_state.jobs.push(queue);

        service
            .handle_backfill_block(None, signed_block, SyncBlockSource::Gossip)
            .await
            .unwrap();

        assert_eq!(service.backfill_state.jobs.len(), 1);
        assert!(!service.backfill_state.jobs[0].is_complete);
        assert_eq!(service.backfill_state.jobs[0].completion_root, None);
        assert!(matches!(
            service.pending_job_requests.front(),
            Some(PendingJobRequest::Initial {
                root,
                slot,
                parent_root,
            }) if *root == last_root && *slot == 3 && *parent_root == missing_root
        ));
    }

    #[tokio::test]
    async fn test_prune_stale_pending_blocks_removes_finalized_orphans() {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let mut pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        {
            pending_block.block.parent_root = B256::repeat_byte(0x77);
        }
        let pending_root = pending_block.block.tree_hash_root();
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_root, pending_block)
            .unwrap();
        store
            .store
            .lock()
            .await
            .latest_finalized_provider()
            .insert(Checkpoint {
                root: B256::repeat_byte(0x55),
                slot: 1,
            })
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        assert!(service.has_orphaned_pending_blocks().await);
        service.prune_stale_pending_blocks().await.unwrap();
        assert!(!service.has_orphaned_pending_blocks().await);
    }

    #[tokio::test]
    async fn test_handle_process_block_routes_missing_parent_to_backfill_path() {
        let mut target_store = sample_store(10).await;

        let block = target_store
            .produce_block_with_signatures(1, 1)
            .await
            .unwrap();
        let mut signed_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        {
            signed_block.block.parent_root = B256::repeat_byte(0x99);
        }
        let block_root = signed_block.block.tree_hash_root();
        let (writer, _reader) = Writer::new(target_store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.sync_status = SyncStatus::Synced;

        service.handle_process_block(&signed_block).await.unwrap();

        let pending_block_exists = {
            let fork_choice = service.store.read().await;
            let store = fork_choice.store.lock().await;
            store
                .pending_blocks_provider()
                .get(block_root)
                .unwrap()
                .is_some()
        };
        assert!(pending_block_exists);
    }

    #[tokio::test]
    async fn test_queue_pending_initial_rebuilds_fresh_queue_when_replace_fails() {
        let store = sample_store(10).await;
        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;

        let peer_id = PeerId::random();
        service.network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );

        let root = B256::repeat_byte(1);
        let parent_root = B256::repeat_byte(2);
        service
            .pending_job_requests
            .push_back(PendingJobRequest::new_initial(root, 99, parent_root));

        service.queue_pending_job_requests().await.unwrap();

        assert!(service.peers_in_use.contains(&peer_id));
        assert!(matches!(
            &service.backfill_state.jobs,
            jobs if jobs.iter().any(|queue|
                queue.starting_root == parent_root
                    && queue.starting_slot == 98
                    && queue.jobs.contains_key(&parent_root)
            )
        ));
    }

    #[tokio::test]
    async fn test_handle_backfill_block_absorbs_older_queue_frontier() {
        let mut store = sample_store(10).await;
        let old_root = B256::repeat_byte(0x11);
        let old_frontier = B256::repeat_byte(0x12);
        let new_root = B256::repeat_byte(0x21);

        let signed_block = {
            let block = store.produce_block_with_signatures(1, 1).await.unwrap();
            let mut signed_block = SignedBlock {
                block: block.block,
                signature: BlockSignatures {
                    attestation_signatures: block.signatures,
                    proposer_signature: Signature::mock(),
                },
            };
            signed_block.block.slot = 101;
            signed_block.block.parent_root = old_root;
            signed_block
        };

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        let peer_old = PeerId::random();
        let peer_new = PeerId::random();

        service.backfill_state.add_new_job_queue(
            Checkpoint {
                root: old_root,
                slot: 120,
            },
            JobRequest::new(peer_old, old_root),
            false,
        );
        service.backfill_state.replace_job_with_next_job(
            old_root,
            61,
            JobRequest::new(peer_old, old_frontier),
        );
        service.peers_in_use.insert(peer_old);

        service.backfill_state.add_new_job_queue(
            Checkpoint {
                root: new_root,
                slot: 140,
            },
            JobRequest::new(peer_new, new_root),
            false,
        );
        let new_frontier = signed_block.block.tree_hash_root();
        service.backfill_state.replace_job_with_next_job(
            new_root,
            101,
            JobRequest::new(peer_new, new_frontier),
        );

        service
            .handle_backfill_block(None, signed_block, SyncBlockSource::ReqResp)
            .await
            .unwrap();

        assert_eq!(service.backfill_state.jobs.len(), 1);
        let surviving_queue = &service.backfill_state.jobs[0];
        assert_eq!(surviving_queue.starting_root, new_root);
        assert!(surviving_queue.jobs.contains_key(&old_frontier));
        assert!(!surviving_queue.jobs.contains_key(&new_frontier));
        assert_eq!(surviving_queue.last_fetched_slot, 61);
        assert!(service.peers_in_use.contains(&peer_old));
    }

    #[tokio::test]
    async fn test_handle_backfill_block_clears_quarantine_for_arrived_root_but_preserves_attempts()
    {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let signed_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        let root = signed_block.block.tree_hash_root();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.backfill_recovery_attempts.insert(root, 3);
        service.quarantined_backfill_roots.insert(
            root,
            super::QuarantinedBackfillRoot {
                slot: 1,
                attempts: 3,
            },
        );
        service.dropped_backfill_roots.insert(root);

        service
            .handle_backfill_block(None, signed_block, SyncBlockSource::Gossip)
            .await
            .unwrap();

        assert_eq!(service.backfill_recovery_attempts.get(&root), Some(&3));
        assert!(!service.quarantined_backfill_roots.contains_key(&root));
        assert!(!service.dropped_backfill_roots.contains(&root));
    }

    #[tokio::test]
    async fn test_handle_forward_sync_completed_clears_recovery_state_for_ending_root() {
        let mut store = sample_store(10).await;
        advance_store_to_slot(&mut store, 3, 10).await;

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        let root = B256::repeat_byte(0xaa);
        let mut previous_queue = JobQueue::new(root, 1, 1);
        previous_queue.is_complete = true;
        service.backfill_state.jobs.push(previous_queue);
        service.backfill_recovery_attempts.insert(root, 3);
        service.quarantined_backfill_roots.insert(
            root,
            super::QuarantinedBackfillRoot {
                slot: 1,
                attempts: 3,
            },
        );
        service.dropped_backfill_roots.insert(root);

        service
            .handle_forward_sync_result(ForwardSyncResults::Completed {
                starting_root: B256::repeat_byte(0x11),
                ending_root: root,
                imported_start_slot: Some(1),
                imported_end_slot: Some(1),
                blocks_synced: 1,
                processing_time_seconds: 0.1,
            })
            .await
            .unwrap();

        assert!(!service.backfill_recovery_attempts.contains_key(&root));
        assert!(!service.quarantined_backfill_roots.contains_key(&root));
        assert!(!service.dropped_backfill_roots.contains(&root));
        assert!(service.backfill_state.jobs.is_empty());
    }

    #[tokio::test]
    async fn test_prune_stale_pending_blocks_preserves_roots_still_referenced_by_backfill() {
        let mut store = sample_store(10).await;
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        let mut pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        {
            pending_block.block.slot = 0;
            pending_block.block.parent_root = B256::ZERO;
        }
        let pending_root = pending_block.block.tree_hash_root();
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(pending_root, pending_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();
        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        service.backfill_state.add_new_job_queue(
            Checkpoint {
                root: pending_root,
                slot: 1,
            },
            JobRequest::new(PeerId::random(), pending_root),
            true,
        );

        service.prune_stale_pending_blocks().await.unwrap();

        let pending_present = {
            let fork_choice = service.store.read().await;
            let store = fork_choice.store.lock().await;
            store
                .pending_blocks_provider()
                .get(pending_root)
                .unwrap()
                .is_some()
        };
        assert!(pending_present);
    }

    #[tokio::test]
    async fn test_handle_callback_response_accepts_late_reassigned_peer_callback() {
        let mut store = sample_store(10).await;

        let signed_block = {
            let block = store.produce_block_with_signatures(1, 1).await.unwrap();
            SignedBlock {
                block: block.block,
                signature: BlockSignatures {
                    attestation_signatures: block.signatures,
                    proposer_signature: Signature::mock(),
                },
            }
        };

        let block_root = signed_block.block.tree_hash_root();

        let (writer, _reader) = Writer::new(store);
        let (_chain_sender, chain_receiver) = mpsc::unbounded_channel();
        let (p2p_sender, _p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

        let mut service = LeanChainService::new(writer, chain_receiver, p2p_sender, false).await;
        let old_peer = PeerId::random();
        let new_peer = PeerId::random();
        service.backfill_state.add_new_job_queue(
            Checkpoint {
                root: block_root,
                slot: 1,
            },
            JobRequest::new(new_peer, block_root),
            false,
        );
        service.peers_in_use.insert(new_peer);

        service
            .handle_callback_response_message(
                old_peer,
                Arc::new(LeanResponseMessage::BlocksByRoot(Arc::new(signed_block))),
            )
            .await
            .unwrap();

        assert!(!service.peers_in_use.contains(&new_peer));
        assert_eq!(service.telemetry.backfill_telemetry.callbacks_processed, 1);
        assert!(!service.backfill_state.contains_job_root(block_root));

        let pending_block_exists = {
            let fork_choice = service.store.read().await;
            let store = fork_choice.store.lock().await;
            store
                .pending_blocks_provider()
                .get(block_root)
                .unwrap()
                .is_some()
        };
        assert!(pending_block_exists);
    }
}
