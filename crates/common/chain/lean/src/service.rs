use std::{
    collections::HashSet,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
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
    },
};

const STATE_RETENTION_SLOTS: u64 = 128;

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
    pending_job_requests: Vec<PendingJobRequest>,
    forward_syncer: Option<JoinHandle<anyhow::Result<ForwardSyncResults>>>,
    checkpoints_to_queue: Vec<(Checkpoint, bool)>,
    pending_callbacks: FuturesUnordered<CallbackFuture>,
}

impl LeanChainService {
    pub async fn new(
        store: LeanStoreWriter,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        outbound_p2p: mpsc::UnboundedSender<LeanP2PRequest>,
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
            pending_job_requests: Vec::new(),
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        info!(
            genesis_time = lean_network_spec().genesis_time,
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
                        self.store.write().await.tick_interval(tick_count % 4 == 1).await.expect("Failed to tick interval");
                        #[cfg(feature = "devnet3")]
                        {
                            // TODO: update is_aggregator logic from devnet config
                            let is_aggregator = true;
                            self.store.write().await.tick_interval(tick_count.is_multiple_of(INTERVALS_PER_SLOT), is_aggregator).await.expect("Failed to tick interval");
                        }
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
                                trace!("Received ProcessBlock request while syncing. Ignoring.");
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
        info!(
            slot = get_current_slot(),
            "Node is syncing; backfill sync step executed",
        );

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
        let unqueued_jobs = self.sync_status.unqueued_jobs();
        for job in unqueued_jobs {
            let (callback, rx) = mpsc::channel(100);
            if let Err(err) = self.outbound_p2p.send(LeanP2PRequest::Request {
                peer_id: job.peer_id,
                callback,
                message: P2PCallbackRequest::BlocksByRoot {
                    roots: vec![job.root],
                },
            }) {
                warn!(
                    "Failed to send block request to peer {:?} for root {:?}: {err:?}",
                    job.peer_id, job.root
                );
                continue;
            }
            self.push_callback_receiver(rx);

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

        match candidates.choose_weighted(&mut rand::rng(), |(_, score)| f64::from(*score)) {
            Ok((peer_id, _)) => Some(*peer_id),
            Err(err) => {
                warn!("Failed to choose weighted peer: {err}");
                None
            }
        }
    }

    async fn update_sync_status(&self) -> anyhow::Result<SyncStatus> {
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

        let sync_status = if is_behind_peers {
            if self.sync_status == SyncStatus::Synced {
                info!(
                    slot = get_current_slot(),
                    head_slot = current_head_slot,
                    peer_head_slot = highest_peer_head_slot,
                    "Node fell behind peers; switching to Syncing"
                );
                SyncStatus::Syncing { jobs: Vec::new() }
            } else {
                self.sync_status.clone()
            }
        } else if is_synced_by_time {
            if self.sync_status != SyncStatus::Synced {
                info!(
                    slot = get_current_slot(),
                    head_slot = current_head_slot,
                    "Node has synced to the head"
                );
            }
            SyncStatus::Synced
        } else {
            if self.sync_status != SyncStatus::Synced {
                info!(
                    slot = get_current_slot(),
                    head_slot = current_head_slot,
                    "Node is behind time but caught up to peers (stall detected); switching to Synced"
                );
            }
            SyncStatus::Synced
        };

        if sync_status == SyncStatus::Synced {
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
        self.peers_in_use.remove(&peer_id);
        self.pending_job_requests
            .push(PendingJobRequest::Reset { peer_id });
        Ok(())
    }

    async fn handle_callback_response_message(
        &mut self,
        peer_id: PeerId,
        message: Arc<LeanResponseMessage>,
    ) -> anyhow::Result<()> {
        self.network_state.successful_response_from_peer(peer_id);

        match &*message {
            LeanResponseMessage::BlocksByRoot(signed_block_with_attestation) => {
                let last_root = signed_block_with_attestation.message.block.tree_hash_root();
                // if the parent root is present in pending blocks or is local head, we mark the
                // queue as complete
                let (head, pending_blocks_provider) = {
                    let fork_choice = self.store.read().await;
                    let store = fork_choice.store.lock().await;
                    (
                        store.head_provider().get()?,
                        store.pending_blocks_provider(),
                    )
                };

                pending_blocks_provider
                    .insert(last_root, signed_block_with_attestation.as_ref().clone())?;
                self.peers_in_use.remove(&peer_id);

                // We have 3 scenarios where we can mark the job queue as complete
                // 1. The parent root is the local head
                // 2. The parent root is already present in the pending blocks (we have already
                //    requested it)
                // 3. The parent root is the starting root of any existing job queue
                let parent_root_is_local_head =
                    signed_block_with_attestation.message.block.parent_root == head;
                let parent_root_in_pending_blocks = pending_blocks_provider
                    .get(signed_block_with_attestation.message.block.parent_root)?
                    .is_some();
                let parent_root_is_start_of_any_queue =
                    self.sync_status.is_root_start_of_any_queue(
                        &signed_block_with_attestation.message.block.parent_root,
                    );
                if parent_root_is_local_head
                    || parent_root_in_pending_blocks
                    || parent_root_is_start_of_any_queue
                {
                    trace!(
                        "Marking job queue as complete for block from peer {peer_id:?} with root {last_root:?}."
                    );
                    self.sync_status.mark_job_queue_as_complete(last_root);

                    return Ok(());
                }

                self.pending_job_requests
                    .push(PendingJobRequest::new_initial(
                        last_root,
                        signed_block_with_attestation.message.block.slot,
                        signed_block_with_attestation.message.block.parent_root,
                    ));

                self.queue_pending_job_requests().await?;
            }
            _ => warn!(
                "We handle these messages elsewhere, received unexpected LeanRequestMessage: {:?}",
                message
            ),
        }
        Ok(())
    }

    async fn queue_pending_job_requests(&mut self) -> anyhow::Result<()> {
        while let Some(pending_job_request) = self.pending_job_requests.pop() {
            let non_queued_peer_id = match self.non_queued_peer_id().await {
                Some(id) => id,
                None => {
                    info!(
                        "No connected peers available to assign pending job request. {:?} || {:?}",
                        self.sync_status, self.pending_job_requests
                    );
                    self.pending_job_requests.push(pending_job_request);
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
            .on_gossip_attestation(signed_attestation, true)
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
