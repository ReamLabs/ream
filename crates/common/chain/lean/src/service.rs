use std::collections::HashMap;

use alloy_primitives::{B256, FixedBytes};
use anyhow::{Context, anyhow};
use ream_consensus_lean::{
    block::{Block, SignedBlock},
    vote::SignedVote,
};
use ream_network_spec::networks::lean_network_spec;
use ream_storage::tables::table::Table;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};
use tree_hash::TreeHash;

use crate::{
    clock::create_lean_clock_interval, lean_chain::LeanChainWriter,
    messages::LeanChainServiceMessage, p2p_request::LeanP2PRequest, queue_item::QueueItem,
    slot::get_current_slot,
};

/// LeanChainService is responsible for updating the [LeanChain] state. `LeanChain` is updated when:
/// 1. Every third (t=2/4) and fourth (t=3/4) ticks.
/// 2. Receiving new blocks or votes from the network.
///
/// NOTE: This service will be the core service to implement `receive()` function.
pub struct LeanChainService {
    lean_chain: LeanChainWriter,
    receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
    sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
    outbound_gossip: mpsc::UnboundedSender<LeanP2PRequest>,
    // Objects that we will process once we have processed their parents
    dependencies: HashMap<B256, Vec<QueueItem>>,
}

impl LeanChainService {
    pub async fn new(
        lean_chain: LeanChainWriter,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
        outbound_gossip: mpsc::UnboundedSender<LeanP2PRequest>,
    ) -> Self {
        LeanChainService {
            lean_chain,
            receiver,
            sender,
            outbound_gossip,
            dependencies: HashMap::new(),
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        info!(
            "LeanChainService started with genesis_time: {}",
            lean_network_spec().genesis_time
        );

        let mut tick_count = 0u64;

        let mut interval =
            create_lean_clock_interval().context("Failed to create clock interval")?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match tick_count % 4 {
                        0 => {
                            // First tick (t=0/4): Log current head state, including its justification/finalization status.
                            let current_slot = get_current_slot();
                            let (head, store) = {
                                let lean_chain = self.lean_chain.read().await;
                                (lean_chain.head, lean_chain.store.clone())
                            };
                            let head_state = store.lock().await
                                .lean_state_provider()
                                .get(head)?.ok_or_else(|| anyhow!("Post state not found for head: {head}"))?;

                            info!(
                                "Current head state of slot {current_slot}: latest_justified.slot: {}, latest_finalized.slot: {}",
                                head_state.latest_justified.slot,
                                head_state.latest_finalized.slot
                            );
                        }
                        2 => {
                            // Third tick (t=2/4): Compute the safe target.
                            let current_slot = get_current_slot();
                            info!("Computing safe target at slot {current_slot} (tick {tick_count})");
                            let mut lean_chain = self.lean_chain.write().await;
                            lean_chain.safe_target = lean_chain.compute_safe_target().await.expect("Failed to compute safe target");
                        }
                        3 => {
                            // Fourth tick (t=3/4): Accept new votes.
                            let current_slot = get_current_slot();
                            info!("Accepting new votes at slot {current_slot} (tick {tick_count})");
                            self.lean_chain.write().await.accept_new_votes().await.expect("Failed to accept new votes");
                        }
                        _ => {
                            // Other ticks (t=0, t=1/4): Do nothing.
                        }
                    }
                    tick_count += 1;
                }
                Some(message) = self.receiver.recv() => {
                    match message {
                        LeanChainServiceMessage::ProduceBlock { slot, sender } => {
                            if let Err(err) = self.handle_produce_block(slot, sender).await {
                                error!("Failed to handle produce block message: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::ProcessBlock { signed_block, is_trusted, need_gossip } => {
                            info!(
                                "Processing block: slot={}, validator_id={}, root={}, parent={}, votes={}",
                                signed_block.message.slot,
                                signed_block.message.proposer_index,
                                signed_block.message.tree_hash_root(),
                                signed_block.message.parent_root,
                                signed_block.message.body.attestations.len(),
                            );

                            if let Err(err) = self.handle_process_block(signed_block.clone(), is_trusted).await {
                                warn!("Failed to handle process block message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_gossip.send(LeanP2PRequest::GossipBlock(signed_block)) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::ProcessVote { signed_vote, is_trusted, need_gossip } => {
                            info!(
                                "Processing vote: slot={}, validator_id={}, source={:?}, target={:?}",
                                signed_vote.message.slot,
                                signed_vote.validator_id,
                                signed_vote.message.source,
                                signed_vote.message.target
                            );

                            if let Err(err) = self.handle_process_vote(signed_vote.clone(), is_trusted).await {
                                warn!("Failed to handle process block message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_gossip.send(LeanP2PRequest::GossipVote(signed_vote)) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    async fn handle_produce_block(
        &mut self,
        slot: u64,
        response: oneshot::Sender<Block>,
    ) -> anyhow::Result<()> {
        let new_block = {
            let mut lean_chain = self.lean_chain.write().await;

            // Accept new votes and modify the lean chain.
            lean_chain.accept_new_votes().await?;

            // Build a block and propose the block.
            lean_chain.propose_block(slot).await?
        };

        // Send the produced block back to the requester
        response
            .send(new_block)
            .map_err(|err| anyhow!("Failed to send produced block: {err:?}"))?;

        Ok(())
    }

    async fn handle_process_block(
        &mut self,
        signed_block: SignedBlock,
        is_trusted: bool,
    ) -> anyhow::Result<()> {
        if !is_trusted {
            // TODO: Validate the signature.
        }

        let block_hash = signed_block.message.tree_hash_root();

        let (lean_block_provider, known_votes_provider) = {
            let lean_chain = self.lean_chain.read().await;
            let db = lean_chain.store.lock().await;
            (db.lean_block_provider(), db.known_votes_provider())
        };

        // If the block is already known, ignore it
        if lean_block_provider.contains_key(block_hash) {
            return Ok(());
        }

        let state = {
            let lean_chain = self.lean_chain.read().await;
            lean_chain
                .store
                .lock()
                .await
                .lean_state_provider()
                .get(signed_block.message.parent_root)?
        };
        match state {
            Some(parent_state) => {
                let mut state = parent_state.clone();
                state.state_transition(&signed_block, true, true)?;

                let mut votes_to_add = Vec::new();
                let mut lean_chain = self.lean_chain.write().await;
                for vote in &signed_block.message.body.attestations {
                    if !known_votes_provider.contains(vote)? {
                        votes_to_add.push(vote.clone());
                    }
                }
                {
                    let db = lean_chain.store.lock().await;
                    db.lean_block_provider()
                        .insert(block_hash, signed_block.clone())?;

                    db.lean_state_provider().insert(block_hash, state)?;

                    db.known_votes_provider().batch_append(votes_to_add)?;
                }

                lean_chain.recompute_head().await?;

                drop(lean_chain);

                // Once we have received a block, also process all of its dependencies
                // by sending them to this service itself.
                // NOTE 1: As we already verified all QueueItems before appending them, we can trust
                // their validity.
                // NOTE 2: We don't need to gossip this, as dependencies are for internal processing
                // only.
                if let Some(queue_items) = self.dependencies.remove(&block_hash) {
                    for item in queue_items {
                        let message = match item {
                            QueueItem::Block(block) => LeanChainServiceMessage::ProcessBlock {
                                signed_block: SignedBlock {
                                    message: block,
                                    signature: FixedBytes::default(),
                                },
                                is_trusted: true,
                                need_gossip: false,
                            },
                            QueueItem::SignedVote(signed_vote) => {
                                LeanChainServiceMessage::ProcessVote {
                                    signed_vote: *signed_vote,
                                    is_trusted: true,
                                    need_gossip: false,
                                }
                            }
                        };

                        self.sender.send(message)?;
                    }
                }
            }
            None => {
                // If we have not yet seen the block's parent, ignore for now,
                // process later once we actually see the parent
                self.dependencies
                    .entry(signed_block.message.parent_root)
                    .or_default()
                    .push(QueueItem::Block(signed_block.message));
            }
        }

        Ok(())
    }

    async fn handle_process_vote(
        &mut self,
        signed_vote: SignedVote,
        is_trusted: bool,
    ) -> anyhow::Result<()> {
        if !is_trusted {
            // TODO: Validate the signature.
        }

        let (lean_block_provider, known_votes_provider) = {
            let lean_chain = self.lean_chain.read().await;
            let db = lean_chain.store.lock().await;
            (db.lean_block_provider(), db.known_votes_provider())
        };

        let is_known_vote = known_votes_provider.contains(&signed_vote)?;
        let is_new_vote = {
            self.lean_chain
                .read()
                .await
                .new_votes
                .contains(&signed_vote)
        };

        if is_known_vote || is_new_vote {
            // Do nothing
        } else if lean_block_provider.contains_key(signed_vote.message.head.root) {
            // We should acquire another write lock
            let mut lean_chain = self.lean_chain.write().await;
            lean_chain.new_votes.push(signed_vote);
        } else {
            self.dependencies
                .entry(signed_vote.message.head.root)
                .or_default()
                .push(QueueItem::SignedVote(Box::new(signed_vote)));
        }

        Ok(())
    }
}
