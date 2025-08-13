use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use anyhow::anyhow;
use ream_consensus_lean::{QueueItem, VoteItem, block::Block, process_block};
use ream_network_spec::networks::lean_network_spec;
use tokio::sync::{RwLock, mpsc};
use tracing::info;
use tree_hash::TreeHash;

use crate::{clock::create_lean_clock_interval, lean_chain::LeanChain, slot::get_current_slot};

#[derive(Debug, Clone)]
pub struct LeanChainServiceMessage {
    pub item: QueueItem,
}

/// LeanChainService is responsible for updating the [LeanChain] state. `LeanChain` is updated when:
/// 1. Every third (t=2/4) and fourth (t=3/4) ticks.
/// 2. Receiving new blocks or votes from the network.
///
/// NOTE: This service will be the core service to implement `receive()` function.
pub struct LeanChainService {
    lean_chain: Arc<RwLock<LeanChain>>,
    receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
    sender: mpsc::UnboundedSender<LeanChainServiceMessage>,

    // Objects that we will process once we have processed their parents
    dependencies: HashMap<B256, Vec<QueueItem>>,
}

impl LeanChainService {
    pub async fn new(
        lean_chain: Arc<RwLock<LeanChain>>,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
    ) -> Self {
        LeanChainService {
            lean_chain,
            receiver,
            sender,
            dependencies: HashMap::new(),
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        info!(
            "LeanChainService started with genesis_time: {}",
            lean_network_spec().genesis_time
        );

        let mut tick_count = 0u64;

        let mut interval = create_lean_clock_interval()
            .map_err(|err| anyhow!("Failed to create clock interval: {err}"))?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match tick_count % 4 {
                        2 => {
                            // Third tick (t=2/4): Compute the safe target.
                            let current_slot = get_current_slot();
                            info!("Computing safe target at slot {current_slot} (tick {tick_count})");
                            let mut lean_chain = self.lean_chain.write().await;
                            lean_chain.safe_target = lean_chain.compute_safe_target().expect("Failed to compute safe target");
                        }
                        3 => {
                            // Fourth tick (t=3/4): Accept new votes.
                            let current_slot = get_current_slot();
                            info!("Accepting new votes at slot {current_slot} (tick {tick_count})");
                            self.lean_chain.write().await.accept_new_votes().expect("Failed to accept new votes");
                        }
                        _ => {
                            // Other ticks (t=0, t=1/4): Do nothing.
                        }
                    }
                    tick_count += 1;
                }
                Some(message) = self.receiver.recv() => {
                    self.handle_message(message).await;
                }
            }
        }
    }

    async fn handle_message(&mut self, message: LeanChainServiceMessage) {
        self.handle_item(message.item).await;
    }

    async fn handle_item(&mut self, item: QueueItem) {
        match item {
            QueueItem::BlockItem(block) => {
                let block_hash = block.tree_hash_root();
                info!(
                    "Received block at slot {} with hash {block_hash:?} from parent {:?}",
                    block.slot, block.parent
                );
                let _ = self.handle_block(block).await;
            }
            QueueItem::VoteItem(vote_item) => {
                match &vote_item {
                    VoteItem::Signed(signed_vote) => {
                        let vote = &signed_vote.data;
                        info!(
                            "Received signed vote from validator {} for head {:?} / source_slot {:?} at slot {}",
                            vote.validator_id, vote.head, vote.source_slot, vote.slot
                        );
                    }
                    VoteItem::Unsigned(vote) => {
                        info!(
                            "Received unsigned vote from validator {} for head {:?} / source_slot {:?} at slot {}",
                            vote.validator_id, vote.head, vote.source_slot, vote.slot
                        );
                    }
                }

                self.handle_vote(vote_item).await;
            }
        }
    }

    async fn handle_block(&mut self, block: Block) -> anyhow::Result<()> {
        let block_hash = block.tree_hash_root();

        let mut lean_chain = self.lean_chain.write().await;

        // If the block is already known, ignore it
        if lean_chain.chain.contains_key(&block_hash) {
            return Ok(());
        }

        match lean_chain.post_states.get(&block.parent) {
            Some(parent_state) => {
                let state = process_block(parent_state, &block)?;

                for vote in &block.votes {
                    if !lean_chain.known_votes.contains(vote) {
                        lean_chain.known_votes.push(vote.clone());
                    }
                }

                lean_chain.chain.insert(block_hash, block);
                lean_chain.post_states.insert(block_hash, state);

                lean_chain.recompute_head()?;

                drop(lean_chain);

                // Once we have received a block, also process all of its dependencies
                // by sending them to this service itself.
                if let Some(queue_items) = self.dependencies.remove(&block_hash) {
                    for item in queue_items {
                        self.sender.send(LeanChainServiceMessage { item })?;
                    }
                }
            }
            None => {
                // If we have not yet seen the block's parent, ignore for now,
                // process later once we actually see the parent
                self.dependencies
                    .entry(block.parent)
                    .or_default()
                    .push(QueueItem::BlockItem(block));
            }
        }

        Ok(())
    }

    async fn handle_vote(&mut self, vote_item: VoteItem) {
        let vote = match vote_item {
            VoteItem::Signed(vote) => {
                // TODO: Validate the signature.
                vote.data
            }
            VoteItem::Unsigned(vote) => vote,
        };

        let lean_chain = self.lean_chain.read().await;
        let is_known_vote = lean_chain.known_votes.contains(&vote);
        let is_new_vote = lean_chain.new_votes.contains(&vote);

        if is_known_vote || is_new_vote {
            // Do nothing
        } else if lean_chain.chain.contains_key(&vote.head) {
            drop(lean_chain);

            // We should acquire another write lock
            let mut lean_chain = self.lean_chain.write().await;
            lean_chain.new_votes.push(vote);
        } else {
            self.dependencies
                .entry(vote.head)
                .or_default()
                .push(QueueItem::VoteItem(VoteItem::Unsigned(vote)));
        }
    }
}
