use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use futures::stream::{FuturesUnordered, StreamExt};
use libp2p_identity::PeerId;
use libp2p_swarm::ConnectionId;
use rand::seq::IndexedRandom;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAttestation},
    block::{BlockWithSignatures, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
};
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
use ream_fork_choice_lean::store::LeanStoreWriter;
use ream_metrics::{CURRENT_SLOT, set_int_gauge_vec};
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
        SyncStatus,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncBlockSource {
    ReqResp,
    Gossip,
}

#[derive(Debug, Clone, Copy)]
struct RecentSyncBlock {
    parent_root: alloy_primitives::B256,
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

#[derive(Debug, Clone, Copy)]
struct InflightRootRequest {
    primary_peer: PeerId,
    backup_peer: Option<PeerId>,
    requested_at: Instant,
    backup_sent: bool,
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
    peers_in_use: HashSet<PeerId>,
    pending_job_requests: VecDeque<PendingJobRequest>,
    forward_syncer: Option<JoinHandle<anyhow::Result<ForwardSyncResults>>>,
    checkpoints_to_queue: Vec<(Checkpoint, bool)>,
    pending_callbacks: FuturesUnordered<CallbackFuture>,
    #[cfg(feature = "devnet3")]
    is_aggregator: bool,
    near_head_backfill_strategy: NearHeadBackfillStrategy,
    near_head_fanout_strategy: NearHeadFanoutStrategy,
    handoff_strategy: HandoffStrategy,
    backfill_timeout_strategy: BackfillTimeoutStrategy,
    pending_dedup_strategy: PendingRequestDedupStrategy,
    peer_selection_strategy: PeerSelectionStrategy,
    recent_sync_blocks: Vec<RecentSyncBlock>,
    callback_loss_mode: CallbackLossMode,
    dropped_callback_roots: HashSet<alloy_primitives::B256>,
    backfill_telemetry: BackfillTelemetry,
    last_backfill_progress_log: Option<Instant>,
    inflight_roots: HashMap<alloy_primitives::B256, InflightRootRequest>,
    peer_avg_latency_ms: HashMap<PeerId, f64>,
}

impl LeanChainService {
    pub async fn new(
        store: LeanStoreWriter,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        outbound_p2p: mpsc::UnboundedSender<LeanP2PRequest>,
        #[cfg(feature = "devnet3")] is_aggregator: bool,
    ) -> Self {
        let network_state = store.read().await.network_state.clone();
        LeanChainService {
            network_state,
            store: Arc::new(store),
            receiver,
            outbound_p2p,
            sync_status: SyncStatus::Syncing { jobs: Vec::new() },
            peers_in_use: HashSet::new(),
            forward_syncer: None,
            checkpoints_to_queue: Vec::new(),
            pending_callbacks: FuturesUnordered::new(),
            pending_job_requests: VecDeque::new(),
            #[cfg(feature = "devnet3")]
            is_aggregator,
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
            inflight_roots: HashMap::new(),
            peer_avg_latency_ms: HashMap::new(),
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        info!(
            genesis_time = lean_network_spec().genesis_time,
            near_head_backfill_strategy = ?self.near_head_backfill_strategy,
            near_head_fanout_strategy = ?self.near_head_fanout_strategy,
            handoff_strategy = ?self.handoff_strategy,
            backfill_timeout_strategy = ?self.backfill_timeout_strategy,
            pending_dedup_strategy = ?self.pending_dedup_strategy,
            peer_selection_strategy = ?self.peer_selection_strategy,
            callback_loss_mode = ?self.callback_loss_mode,
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
                        #[cfg(feature = "devnet2")]
                        self.store.write().await.tick_interval(tick_count % INTERVALS_PER_SLOT == 1).await.expect("Failed to tick interval");
                        #[cfg(feature = "devnet3")]
                        self.store.write().await.tick_interval(tick_count.is_multiple_of(INTERVALS_PER_SLOT), self.is_aggregator).await.expect("Failed to tick interval");
                        self.step_head_sync(tick_count).await?;
                    }

                    tick_count += 1;
                }
                _ = sync_interval.tick(), if self.sync_status != SyncStatus::Synced && genesis_passed => {
                    self.step_backfill_sync().await?;
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

                    match forward_syncer {
                        ForwardSyncResults::Completed { starting_root, ending_root, blocks_synced, processing_time_seconds } => {
                            info!(
                                starting_root = ?starting_root,
                                ending_root = ?ending_root,
                                blocks_synced,
                                processing_time_seconds,
                                "Forward background sync completed",
                            );
                            // The ending root is the starting root of the processed queue, since
                            // the sync walks backwards from there to the head.
                            self.sync_status.remove_processed_queue(ending_root);
                        }
                        ForwardSyncResults::ChainIncomplete { prevous_queue, checkpoint_for_new_queue } => {
                            warn!(
                                starting_root = ?prevous_queue.starting_root,
                                starting_slot = prevous_queue.starting_slot,
                                "Forward background sync incomplete; re-queuing job",
                            );
                            self.checkpoints_to_queue.push((checkpoint_for_new_queue, true));
                        }
                    }
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
                            if let SyncStatus::Syncing { .. } = self.sync_status {
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
                            if let SyncStatus::Syncing { .. } = self.sync_status {
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
                        LeanChainServiceMessage::ProcessBlock { signed_block_with_attestation, need_gossip } => {
                            if self.sync_status != SyncStatus::Synced {
                                if let Err(err) = self
                                    .handle_syncing_process_block(&signed_block_with_attestation)
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
                                    slot = signed_block_with_attestation.message.block.slot,
                                    block_root = ?signed_block_with_attestation.message.block.tree_hash_root(),
                                    parent_root = ?signed_block_with_attestation.message.block.parent_root,
                                    state_root = ?signed_block_with_attestation.message.block.state_root,
                                    attestations_length = signed_block_with_attestation.message.block.body.attestations.len(),
                                    "Processing block built by Validator {}",
                                    signed_block_with_attestation.message.block.proposer_index,
                                );
                            } else {
                                info!(
                                    slot = signed_block_with_attestation.message.block.slot,
                                    block_root = ?signed_block_with_attestation.message.block.tree_hash_root(),
                                    "Processing block built by Validator {}",
                                    signed_block_with_attestation.message.block.proposer_index,
                                );
                            }

                            if let Err(err) = self.handle_process_block(&signed_block_with_attestation).await {
                                warn!("Failed to handle process block message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_p2p.send(LeanP2PRequest::GossipBlock(signed_block_with_attestation)) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }
                        #[cfg(feature = "devnet2")]
                        LeanChainServiceMessage::ProcessAttestation { signed_attestation, need_gossip } => {
                            if self.sync_status != SyncStatus::Synced {
                                trace!("Received ProcessAttestation request while syncing. Ignoring.");
                                continue;
                            }

                            debug!(
                                slot = signed_attestation.message.slot,
                                head = ?signed_attestation.message.head,
                                source = ?signed_attestation.message.source,
                                target = ?signed_attestation.message.target,
                                "Processing attestation by Validator {}",
                                signed_attestation.validator_id,
                            );

                            if let Err(err) = self.handle_process_attestation(*signed_attestation.clone()).await {
                                warn!("Failed to handle process attestation message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_p2p.send(LeanP2PRequest::GossipAttestation(signed_attestation)) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }
                        #[cfg(feature = "devnet3")]
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
                        #[cfg(feature = "devnet3")]
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
            #[cfg(feature = "devnet2")]
            2 => {
                // Third tick (devnet2): Compute the safe target.
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
            #[cfg(feature = "devnet2")]
            3 => {
                // Fourth tick (devnet2): Accept new attestations.
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
            #[cfg(feature = "devnet3")]
            3 => {
                // Fourth tick (devnet3): Compute the safe target.
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
            #[cfg(feature = "devnet3")]
            4 => {
                // Fifth tick (devnet3): Accept new attestations.
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
        self.maybe_log_backfill_progress();
        self.prune_recent_sync_blocks();
        let backfill_job_timeout = self.current_backfill_job_timeout().await;
        for timed_out_job in self.sync_status.reset_timed_out_jobs(backfill_job_timeout) {
            self.backfill_telemetry.request_retries += 1;
            self.inflight_roots.remove(&timed_out_job.root);
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

        // If a queue has reached the stored head, execute that queue in a background thread,
        // blocking any other threads from processing until it returns. The thread can
        // return early and start a new queue if it finds that It can't walk back to the stored
        // head.
        if self.forward_syncer.is_none()
            && let Some(earliest_complete_queue) = self.sync_status.get_ready_to_process_queue()
        {
            let store = self.store.clone();
            let network_state = self.network_state.clone();
            info!(
                "Starting forward background syncer for completed job queue starting at root {:?} and slot {}",
                earliest_complete_queue.starting_root, earliest_complete_queue.starting_slot
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
            self.near_head_fanout_strategy,
            peer_gap_slots,
            NEAR_HEAD_FANOUT_MAX_GAP_SLOTS,
        );
        self.queue_pending_job_requests().await?;
        self.process_delayed_hedges();
        let unqueued_jobs = self.sync_status.unqueued_jobs();
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
                self.near_head_backfill_strategy,
                NearHeadBackfillStrategy::GossipPreferred
            ) && self.try_advance_job_with_cached_block(job.root).await?
            {
                continue;
            }

            if self.inflight_roots.contains_key(&job.root) {
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
            if self.near_head_fanout_strategy == NearHeadFanoutStrategy::DualPeer
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
            self.inflight_roots.insert(job.root, inflight_request);

            self.sync_status.mark_job_as_requested(job.root);
        }

        self.queue_pending_job_requests().await?;

        // start new queue from peers status
        let common_highest_checkpoint = match self.network_state.common_highest_checkpoint() {
            Some(checkpoint) => checkpoint,
            None => {
                warn!("No common highest checkpoint found among connected peers.");
                return Ok(());
            }
        };

        if self
            .sync_status
            .slot_is_subset_of_any_queue(common_highest_checkpoint.slot)
            || self
                .checkpoints_to_queue
                .iter()
                .any(|(checkpoint, _)| checkpoint.slot == common_highest_checkpoint.slot)
        {
            return Ok(());
        }

        self.checkpoints_to_queue
            .push((common_highest_checkpoint, false));

        while let Some((checkpoint, bypass_slot_check)) = self.checkpoints_to_queue.pop() {
            let non_queued_peer_id = match self.non_queued_peer_id().await {
                Some(id) => id,
                None => {
                    if self.network_state.connected_peer_count() == 0 {
                        info!("No connected peers available to start new job queue.");
                    } else {
                        info!("All connected peers are currently in use for syncing.");
                    }
                    self.checkpoints_to_queue
                        .push((checkpoint, bypass_slot_check));
                    return Ok(());
                }
            };
            let new_queue_added = self.sync_status.add_new_job_queue(
                checkpoint,
                JobRequest::new(non_queued_peer_id, checkpoint.root),
                bypass_slot_check,
            );
            if new_queue_added {
                self.peers_in_use.insert(non_queued_peer_id);
            }
        }

        Ok(())
    }

    async fn non_queued_peer_id(&self) -> Option<PeerId> {
        let candidates: Vec<(PeerId, u8)> = self
            .network_state
            .connected_peer_ids_with_scores()
            .into_iter()
            .filter(|(peer_id, _)| !self.peers_in_use.contains(peer_id))
            .collect();

        match candidates.choose_weighted(&mut rand::rng(), |(peer_id, score)| {
            self.peer_weight(*peer_id, *score)
        }) {
            Ok((peer_id, _)) => Some(*peer_id),
            Err(err) => {
                warn!("Failed to choose weighted peer: {err}");
                None
            }
        }
    }

    fn alternate_peer_for_fanout(&self, primary_peer_id: PeerId) -> Option<PeerId> {
        let candidates: Vec<(PeerId, u8)> = self
            .network_state
            .connected_peer_ids_with_scores()
            .into_iter()
            .filter(|(peer_id, _)| *peer_id != primary_peer_id)
            .collect();

        match candidates.choose_weighted(&mut rand::rng(), |(peer_id, score)| {
            self.peer_weight(*peer_id, *score)
        }) {
            Ok((peer_id, _)) => Some(*peer_id),
            Err(_) => None,
        }
    }

    fn peer_weight(&self, peer_id: PeerId, score: u8) -> f64 {
        let score_weight = f64::from(score.max(1));
        match self.peer_selection_strategy {
            PeerSelectionStrategy::ScoreOnly => score_weight,
            PeerSelectionStrategy::LatencyWeighted => {
                let latency_penalty = self
                    .peer_avg_latency_ms
                    .get(&peer_id)
                    .map(|latency_ms| 1.0 / (1.0 + (latency_ms / 1500.0)))
                    .unwrap_or(1.0);
                (score_weight * latency_penalty).max(0.1)
            }
        }
    }

    fn request_block_by_root_from_peer(
        &mut self,
        peer_id: PeerId,
        root: alloy_primitives::B256,
    ) -> bool {
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
        self.backfill_telemetry.requests_sent += 1;
        true
    }

    fn process_delayed_hedges(&mut self) {
        if self.near_head_fanout_strategy != NearHeadFanoutStrategy::DelayedHedge {
            return;
        }

        let now = Instant::now();
        let roots_to_hedge: Vec<(alloy_primitives::B256, PeerId, PeerId)> = self
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
                if let Some(inflight) = self.inflight_roots.get_mut(&root) {
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
                Ok(Some(block)) => block.message.block.slot,
                _ => return 0,
            }
        };
        let highest_peer_head_slot = self
            .network_state
            .common_highest_checkpoint()
            .map(|checkpoint| checkpoint.slot)
            .unwrap_or(local_head_slot);
        highest_peer_head_slot.saturating_sub(local_head_slot)
    }

    async fn update_sync_status(&mut self) -> anyhow::Result<SyncStatus> {
        if self.forward_syncer.is_some() {
            return Ok(self.sync_status.clone());
        }

        let (head, block_provider) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (store.head_provider().get()?, store.block_provider())
        };
        let current_head_slot = block_provider
            .get(head)?
            .ok_or_else(|| anyhow!("Block not found for head: {head}"))?
            .message
            .block
            .slot;

        let tolerance = std::cmp::max(8, (lean_network_spec().num_validators * 2) / 3);
        let highest_peer_head_slot = self
            .network_state
            .common_highest_checkpoint()
            .map(|c| c.slot)
            .unwrap_or(0);
        let is_synced_by_time = get_current_slot() <= current_head_slot + tolerance;
        let is_behind_peers = highest_peer_head_slot > current_head_slot + 2;
        let has_pending_backfill_work = self.has_pending_backfill_work();
        let has_active_backfill_jobs = self.has_active_backfill_jobs();
        let has_inflight_backfill_requests = !self.inflight_roots.is_empty();
        let has_near_head_bridge = self.has_recent_near_head_gossip_bridge(
            head,
            current_head_slot,
            highest_peer_head_slot,
        );
        let should_be_synced = should_switch_to_synced(
            self.handoff_strategy,
            HandoffInputs {
                is_behind_peers,
                has_pending_backfill_work,
                has_near_head_bridge,
                has_active_backfill_jobs,
                has_inflight_backfill_requests,
            },
        );

        let sync_status = if should_be_synced {
            if self.sync_status != SyncStatus::Synced {
                if is_synced_by_time {
                    info!(
                        slot = get_current_slot(),
                        head_slot = current_head_slot,
                        "Node has synced to the head"
                    );
                } else {
                    info!(
                        slot = get_current_slot(),
                        head_slot = current_head_slot,
                        "Node is behind time but caught up to peers (stall detected); switching to Synced"
                    );
                }
            }
            SyncStatus::Synced
        } else {
            if self.sync_status == SyncStatus::Synced {
                info!(
                    slot = get_current_slot(),
                    head_slot = current_head_slot,
                    peer_head_slot = highest_peer_head_slot,
                    has_pending_backfill_work,
                    has_near_head_bridge,
                    has_active_backfill_jobs,
                    has_inflight_backfill_requests,
                    handoff_strategy = ?self.handoff_strategy,
                    "Node remains in backfill syncing mode"
                );
                SyncStatus::Syncing { jobs: Vec::new() }
            } else {
                self.sync_status.clone()
            }
        };

        if sync_status == SyncStatus::Synced {
            self.dropped_callback_roots.clear();
            self.inflight_roots.clear();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|err| anyhow!("System time before epoch: {err:?}"))?
                .as_secs();
            #[cfg(feature = "devnet2")]
            self.store.write().await.on_tick(now, false).await?;
            #[cfg(feature = "devnet3")]
            self.store.write().await.on_tick(now, false, true).await?;
        }

        Ok(sync_status)
    }

    async fn current_backfill_job_timeout(&self) -> Duration {
        let peer_gap = self.current_peer_gap_slots().await;
        self.backfill_timeout_strategy
            .timeout_for_peer_gap(peer_gap)
    }

    fn has_pending_backfill_work(&self) -> bool {
        let has_queued_jobs =
            matches!(&self.sync_status, SyncStatus::Syncing { jobs } if !jobs.is_empty());
        let has_busy_peers = has_queued_jobs && !self.peers_in_use.is_empty();
        has_queued_jobs
            || !self.pending_job_requests.is_empty()
            || !self.checkpoints_to_queue.is_empty()
            || has_busy_peers
            || self.forward_syncer.is_some()
    }

    fn has_active_backfill_jobs(&self) -> bool {
        matches!(
            &self.sync_status,
            SyncStatus::Syncing { jobs } if jobs.iter().any(|queue| !queue.jobs.is_empty())
        )
    }

    fn sync_queue_stats(&self) -> (usize, usize) {
        if let SyncStatus::Syncing { jobs } = &self.sync_status {
            let queue_count = jobs.len();
            let total_jobs = jobs.iter().map(|queue| queue.jobs.len()).sum();
            return (queue_count, total_jobs);
        }
        (0, 0)
    }

    fn maybe_log_backfill_progress(&mut self) {
        let now = Instant::now();
        if let Some(last_log_time) = self.last_backfill_progress_log
            && now.saturating_duration_since(last_log_time) < BACKFILL_PROGRESS_LOG_INTERVAL
        {
            return;
        }
        self.last_backfill_progress_log = Some(now);

        let (queue_count, total_jobs) = self.sync_queue_stats();
        let avg_callback_latency_ms = if self.backfill_telemetry.callback_latency_samples == 0 {
            0.0
        } else {
            self.backfill_telemetry.callback_latency_ms_total as f64
                / self.backfill_telemetry.callback_latency_samples as f64
        };
        info!(
            slot = get_current_slot(),
            queue_count,
            total_jobs,
            pending_requests = self.pending_job_requests.len(),
            inflight_roots = self.inflight_roots.len(),
            peers_in_use = self.peers_in_use.len(),
            recent_sync_blocks = self.recent_sync_blocks.len(),
            requests_sent = self.backfill_telemetry.requests_sent,
            request_retries = self.backfill_telemetry.request_retries,
            callbacks_processed = self.backfill_telemetry.callbacks_processed,
            callbacks_dropped = self.backfill_telemetry.callbacks_dropped,
            avg_callback_latency_ms,
            peer_latency_entries = self.peer_avg_latency_ms.len(),
            "Node is syncing; backfill progress"
        );
    }

    fn queue_pending_reset(&mut self, peer_id: PeerId) {
        if self.pending_dedup_strategy == PendingRequestDedupStrategy::Dedup
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

    fn queue_pending_initial(
        &mut self,
        root: alloy_primitives::B256,
        slot: u64,
        parent_root: alloy_primitives::B256,
    ) {
        if self.pending_dedup_strategy == PendingRequestDedupStrategy::Dedup
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

    fn has_recent_near_head_gossip_bridge(
        &self,
        head: alloy_primitives::B256,
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
        self.recent_sync_blocks.iter().any(|block| {
            block.source == SyncBlockSource::Gossip
                && now.saturating_duration_since(block.seen_at) <= RECENT_SYNC_BLOCK_RETENTION
                && block.parent_root == head
                && block.slot > current_head_slot
                && block.slot <= highest_peer_head_slot.saturating_add(1)
        })
    }

    fn record_recent_sync_block(
        &mut self,
        parent_root: alloy_primitives::B256,
        slot: u64,
        source: SyncBlockSource,
    ) {
        self.recent_sync_blocks.push(RecentSyncBlock {
            parent_root,
            slot,
            seen_at: Instant::now(),
            source,
        });
        self.prune_recent_sync_blocks();
    }

    fn prune_recent_sync_blocks(&mut self) {
        let now = Instant::now();
        self.recent_sync_blocks.retain(|block| {
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
                for root in blocks_by_root_v1_request.inner {
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
        if !self.sync_status.has_job_for_peer(peer_id) {
            return Ok(());
        }
        self.peers_in_use.remove(&peer_id);
        self.inflight_roots.retain(|_, inflight| {
            inflight.primary_peer != peer_id && inflight.backup_peer != Some(peer_id)
        });
        self.queue_pending_reset(peer_id);
        Ok(())
    }

    async fn handle_callback_response_message(
        &mut self,
        peer_id: PeerId,
        message: Arc<LeanResponseMessage>,
    ) -> anyhow::Result<()> {
        match &*message {
            LeanResponseMessage::BlocksByRoot(signed_block_with_attestation) => {
                let block_root = signed_block_with_attestation.message.block.tree_hash_root();
                if !self.inflight_roots.contains_key(&block_root)
                    && !self.sync_status.contains_job_root(block_root)
                {
                    trace!(
                        peer_id = ?peer_id,
                        block_root = ?block_root,
                        "Ignoring stale backfill callback for completed root"
                    );
                    return Ok(());
                }
                if self.should_drop_callback_response(block_root) {
                    self.backfill_telemetry.callbacks_dropped += 1;
                    warn!(
                        peer_id = ?peer_id,
                        block_root = ?block_root,
                        callback_loss_mode = ?self.callback_loss_mode,
                        "Dropping req/resp block callback to simulate packet loss"
                    );
                    return Ok(());
                }
                self.handle_backfill_block(
                    Some(peer_id),
                    signed_block_with_attestation.as_ref().clone(),
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

    fn should_drop_callback_response(&mut self, root: alloy_primitives::B256) -> bool {
        match self.callback_loss_mode {
            CallbackLossMode::None => false,
            CallbackLossMode::DropFirstPerRoot => self.dropped_callback_roots.insert(root),
        }
    }

    async fn handle_syncing_process_block(
        &mut self,
        signed_block_with_attestation: &SignedBlockWithAttestation,
    ) -> anyhow::Result<()> {
        let root = signed_block_with_attestation.message.block.tree_hash_root();
        trace!(
            root = ?root,
            slot = signed_block_with_attestation.message.block.slot,
            "Received gossiped block while backfill syncing"
        );
        self.handle_backfill_block(
            None,
            signed_block_with_attestation.clone(),
            SyncBlockSource::Gossip,
        )
        .await
    }

    async fn try_advance_job_with_cached_block(
        &mut self,
        root: alloy_primitives::B256,
    ) -> anyhow::Result<bool> {
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
        signed_block_with_attestation: SignedBlockWithAttestation,
        source: SyncBlockSource,
    ) -> anyhow::Result<()> {
        let last_root = signed_block_with_attestation.message.block.tree_hash_root();
        let parent_root = signed_block_with_attestation.message.block.parent_root;
        let slot = signed_block_with_attestation.message.block.slot;
        self.inflight_roots.remove(&last_root);
        let mut request_latency_ms: Option<f64> = None;
        if source == SyncBlockSource::ReqResp {
            self.backfill_telemetry.callbacks_processed += 1;
            if let Some(latency) = self.sync_status.request_latency_for_root(last_root) {
                self.backfill_telemetry.callback_latency_ms_total += latency.as_millis();
                self.backfill_telemetry.callback_latency_samples += 1;
                request_latency_ms = Some(latency.as_secs_f64() * 1_000.0);
            }
        }
        let job_peer_id = self.sync_status.peer_for_job_root(last_root);
        let (head, pending_blocks_provider, block_provider) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (
                store.head_provider().get()?,
                store.pending_blocks_provider(),
                store.block_provider(),
            )
        };
        pending_blocks_provider.insert(last_root, signed_block_with_attestation)?;
        self.record_recent_sync_block(parent_root, slot, source);

        if let Some(job_peer_id) = job_peer_id {
            self.peers_in_use.remove(&job_peer_id);
        }

        if let Some(peer_id) = source_peer_id {
            self.network_state.successful_response_from_peer(peer_id);
            if let Some(latency_ms) = request_latency_ms {
                self.peer_avg_latency_ms
                    .entry(peer_id)
                    .and_modify(|avg_ms| *avg_ms = (*avg_ms * 0.8) + (latency_ms * 0.2))
                    .or_insert(latency_ms);
            }
            self.peers_in_use.remove(&peer_id);
        }

        let parent_root_is_local_head = parent_root == head;
        let parent_root_in_pending_blocks = pending_blocks_provider.get(parent_root)?.is_some();
        let parent_root_in_block_store = block_provider.get(parent_root)?.is_some();
        let parent_root_is_start_of_any_queue =
            self.sync_status.is_root_start_of_any_queue(&parent_root);
        if parent_root_is_local_head
            || parent_root_in_pending_blocks
            || parent_root_in_block_store
            || parent_root_is_start_of_any_queue
        {
            trace!(
                root = ?last_root,
                parent_root = ?parent_root,
                "Marking backfill queue as complete"
            );
            self.sync_status.mark_job_queue_as_complete(last_root);
            return Ok(());
        }

        if self.sync_status.contains_job_root(last_root) {
            self.queue_pending_initial(last_root, slot, parent_root);
            self.queue_pending_job_requests().await?;
        }

        Ok(())
    }

    async fn queue_pending_job_requests(&mut self) -> anyhow::Result<()> {
        while let Some(pending_job_request) = self.pending_job_requests.pop_front() {
            let non_queued_peer_id = match self.non_queued_peer_id().await {
                Some(id) => id,
                None => {
                    info!(
                        "No connected peers available to assign pending job request. {:?} || {:?}",
                        self.sync_status, self.pending_job_requests
                    );
                    self.pending_job_requests.push_back(pending_job_request);
                    return Ok(());
                }
            };
            match pending_job_request {
                PendingJobRequest::Reset { peer_id } => {
                    if self
                        .sync_status
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
                        .sync_status
                        .replace_job_with_next_job(
                            root,
                            slot,
                            JobRequest::new(non_queued_peer_id, parent_root),
                        )
                        .is_none()
                    {
                        warn!(
                            "Failed to add initial pending job request for root {root:?} - job may already exist.",
                        );
                        continue;
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
            .message
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
        let block_with_signatures = self
            .store
            .write()
            .await
            .produce_block_with_signatures(slot, slot % lean_network_spec().num_validators)
            .await?;

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
        let attestation_data = self
            .store
            .read()
            .await
            .produce_attestation_data(slot)
            .await?;

        response
            .send(ServiceResponse::Ok(attestation_data))
            .map_err(|err| anyhow!("Failed to send built attestation data: {err:?}"))?;

        Ok(())
    }

    async fn handle_process_block(
        &mut self,
        signed_block_with_attestation: &SignedBlockWithAttestation,
    ) -> anyhow::Result<()> {
        self.store
            .write()
            .await
            .on_block(signed_block_with_attestation, true)
            .await?;

        Ok(())
    }

    async fn handle_process_attestation(
        &mut self,
        signed_attestation: SignedAttestation,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "devnet2")]
        self.store
            .write()
            .await
            .on_gossip_attestation(signed_attestation)
            .await?;

        #[cfg(feature = "devnet3")]
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
