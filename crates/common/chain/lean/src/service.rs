use anyhow::{Context, anyhow};
use ream_consensus_lean::{
    block::{Block, SignedBlock},
    vote::SignedAttestation,
};
use ream_network_spec::networks::lean_network_spec;
use ream_storage::tables::table::Table;
use tokio::sync::{mpsc, oneshot};
use tracing::{Level, debug, enabled, error, info, warn};
use tree_hash::TreeHash;

use crate::{
    clock::create_lean_clock_interval, lean_chain::LeanChainWriter,
    messages::LeanChainServiceMessage, p2p_request::LeanP2PRequest, slot::get_current_slot,
};

/// LeanChainService is responsible for updating the [LeanChain] state. `LeanChain` is updated when:
/// 1. Every third (t=2/4) and fourth (t=3/4) ticks.
/// 2. Receiving new blocks or votes from the network.
///
/// NOTE: This service will be the core service to implement `receive()` function.
pub struct LeanChainService {
    lean_chain: LeanChainWriter,
    receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
    outbound_gossip: mpsc::UnboundedSender<LeanP2PRequest>,
}

impl LeanChainService {
    pub async fn new(
        lean_chain: LeanChainWriter,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        outbound_gossip: mpsc::UnboundedSender<LeanP2PRequest>,
    ) -> Self {
        LeanChainService {
            lean_chain,
            receiver,
            outbound_gossip,
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        info!(
            genesis_time = lean_network_spec().genesis_time,
            "LeanChainService started",
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
                            let (head, store) = {
                                let lean_chain = self.lean_chain.read().await;
                                (lean_chain.head, lean_chain.store.clone())
                            };
                            let head_state = store.lock().await
                                .lean_state_provider()
                                .get(head)?.ok_or_else(|| anyhow!("Post state not found for head: {head}"))?;

                            info!(
                                slot = get_current_slot(),
                                justified_slot = head_state.latest_justified.slot,
                                finalized_slot = head_state.latest_finalized.slot,
                                "Current head state information",
                            );
                        }
                        2 => {
                            // Third tick (t=2/4): Compute the safe target.
                            info!(
                                slot = get_current_slot(),
                                tick = tick_count,
                                "Computing safe target"
                            );
                            self.lean_chain.write().await.update_safe_target().await.expect("Failed to update safe target");
                        }
                        3 => {
                            // Fourth tick (t=3/4): Accept new votes.
                            info!(
                                slot = get_current_slot(),
                                tick = tick_count,
                                "Accepting new votes"
                            );
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
                            if enabled!(Level::DEBUG) {
                                debug!(
                                    slot = signed_block.message.slot,
                                    block_root = ?signed_block.message.tree_hash_root(),
                                    parent_root = ?signed_block.message.parent_root,
                                    state_root = ?signed_block.message.state_root,
                                    attestations_length = signed_block.message.body.attestations.len(),
                                    "Processing block built by Validator {}",
                                    signed_block.message.proposer_index,
                                );
                            } else {
                                info!(
                                    slot = signed_block.message.slot,
                                    block_root = ?signed_block.message.tree_hash_root(),
                                    "Processing block built by Validator {}",
                                    signed_block.message.proposer_index,
                                );
                            }

                            if let Err(err) = self.handle_process_block(signed_block.clone(), is_trusted).await {
                                warn!("Failed to handle process block message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_gossip.send(LeanP2PRequest::GossipBlock(signed_block)) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::ProcessVote { signed_vote, is_trusted, need_gossip } => {
                            if enabled!(Level::DEBUG) {
                                debug!(
                                    slot = signed_vote.message.slot(),
                                    head = ?signed_vote.message.head(),
                                    source = ?signed_vote.message.source(),
                                    target = ?signed_vote.message.target(),
                                    "Processing vote by Validator {}",
                                    signed_vote.message.validator_id,
                                );
                            } else {
                                info!(
                                    slot = signed_vote.message.slot(),
                                    source_slot = signed_vote.message.source().slot,
                                    target_slot = signed_vote.message.target().slot,
                                    "Processing vote by Validator {}",
                                    signed_vote.message.validator_id,
                                );
                            }

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
        let new_block = self.lean_chain.write().await.propose_block(slot).await?;

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

        self.lean_chain
            .write()
            .await
            .on_block(signed_block.clone())
            .await?;

        Ok(())
    }

    async fn handle_process_vote(
        &mut self,
        signed_vote: SignedAttestation,
        is_trusted: bool,
    ) -> anyhow::Result<()> {
        if !is_trusted {
            // TODO: Validate the signature.
        }

        self.lean_chain
            .write()
            .await
            .on_attestation_from_gossip(signed_vote);

        Ok(())
    }
}
