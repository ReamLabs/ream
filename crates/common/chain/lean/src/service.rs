use std::collections::HashMap;

use alloy_primitives::B256;
use anyhow::anyhow;
use ream_consensus_lean::{VoteItem, block::Block, process_block};
use ream_network_spec::networks::lean_network_spec;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tree_hash::TreeHash;

use crate::{
    clock::create_lean_clock_interval,
    lean_chain::LeanChainWriter,
    messages::{LeanChainServiceMessage, ProduceBlockMessage, QueueItem},
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
    outbound_gossip: mpsc::UnboundedSender<QueueItem>,
    // Objects that we will process once we have processed their parents
    dependencies: HashMap<B256, Vec<QueueItem>>,
}

impl LeanChainService {
    pub async fn new(
        lean_chain: LeanChainWriter,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
        outbound_gossip: mpsc::UnboundedSender<QueueItem>,
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
                    match message {
                        LeanChainServiceMessage::ProduceBlock(produce_block_message) => {
                            if let Err(err) = self.handle_produce_block(produce_block_message).await {
                                error!("Failed to handle produce block message: {err}");
                            }
                        }
                        LeanChainServiceMessage::QueueItem(queue_item) => {
                            self.handle_queue_item(queue_item.clone()).await;
                            if let Err(err) = self.outbound_gossip.send(queue_item) {
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
        produce_block_message: ProduceBlockMessage,
    ) -> anyhow::Result<()> {
        let slot = produce_block_message.slot;
        let new_block = {
            let mut lean_chain = self.lean_chain.write().await;

            // Accept new votes and modify the lean chain.
            lean_chain.accept_new_votes()?;

            // Build a block and propose the block.
            lean_chain.propose_block(slot)?
        };

        // Send the produced block back to the requester
        produce_block_message
            .response
            .send(new_block)
            .map_err(|err| anyhow!("Failed to send produced block: {err:?}"))?;

        Ok(())
    }

    async fn handle_queue_item(&mut self, item: QueueItem) {
        match item {
            QueueItem::Block(block) => {
                let block_hash = block.tree_hash_root();
                info!(
                    "Received block at slot {} with hash {block_hash:?} from parent {:?}",
                    block.slot, block.parent
                );
                let _ = self.handle_block(block).await;
            }
            QueueItem::Vote(vote_item) => {
                match &vote_item {
                    VoteItem::Signed(signed_vote) => {
                        let vote = &signed_vote.data;
                        info!(
                            "Received signed vote from validator {} for head {:?} / source_slot {:?} at slot {}",
                            vote.validator_id, vote.head, vote.source.slot, vote.slot
                        );
                    }
                    VoteItem::Unsigned(vote) => {
                        info!(
                            "Received unsigned vote from validator {} for head {:?} / source_slot {:?} at slot {}",
                            vote.validator_id, vote.head, vote.source.slot, vote.slot
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
                        self.sender.send(LeanChainServiceMessage::QueueItem(item))?;
                    }
                }
            }
            None => {
                // If we have not yet seen the block's parent, ignore for now,
                // process later once we actually see the parent
                self.dependencies
                    .entry(block.parent)
                    .or_default()
                    .push(QueueItem::Block(block));
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
        } else if lean_chain.chain.contains_key(&vote.head.root) {
            drop(lean_chain);

            // We should acquire another write lock
            let mut lean_chain = self.lean_chain.write().await;
            lean_chain.new_votes.push(vote);
        } else {
            self.dependencies
                .entry(vote.head.root)
                .or_default()
                .push(QueueItem::Vote(VoteItem::Unsigned(vote)));
        }
    }
}
