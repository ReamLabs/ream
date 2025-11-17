use anyhow::anyhow;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAttestation},
    block::{BlockWithSignatures, SignedBlockWithAttestation},
};
use ream_fork_choice_lean::store::LeanStoreWriter;
use ream_network_spec::networks::lean_network_spec;
use ream_storage::tables::{field::REDBField, table::REDBTable};
use tokio::sync::{mpsc, oneshot};
use tracing::{Level, debug, enabled, error, info, warn};
use tree_hash::TreeHash;

use crate::{
    clock::create_lean_clock_interval, messages::LeanChainServiceMessage,
    p2p_request::LeanP2PRequest, slot::get_current_slot,
};

/// LeanChainService is responsible for updating the [LeanChain] state. `LeanChain` is updated when:
/// 1. Every third (t=2/4) and fourth (t=3/4) ticks.
/// 2. Receiving new blocks or attestations from the network.
///
/// NOTE: This service will be the core service to implement `receive()` function.
pub struct LeanChainService {
    store: LeanStoreWriter,
    receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
    outbound_gossip: mpsc::UnboundedSender<LeanP2PRequest>,
}

impl LeanChainService {
    pub async fn new(
        store: LeanStoreWriter,
        receiver: mpsc::UnboundedReceiver<LeanChainServiceMessage>,
        outbound_gossip: mpsc::UnboundedSender<LeanP2PRequest>,
    ) -> Self {
        LeanChainService {
            store,
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

        let mut interval = create_lean_clock_interval()
            .map_err(|err| anyhow!("Failed to create clock interval: {err:?}"))?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.store.write().await.tick_interval(tick_count % 4 == 1).await.expect("Failed to tick interval");
                    match tick_count % 4 {
                        0 => {
                            // First tick (t=0/4): Log current head state, including its justification/finalization status.
                            let (head, store) = {
                                let store = self.store.read().await;
                                (store.store.lock().await.lean_head_provider().get()?, store.store.clone())
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
                            self.store.write().await.update_safe_target().await.expect("Failed to update safe target");
                        }
                        3 => {
                            // Fourth tick (t=3/4): Accept new attestations.
                            info!(
                                slot = get_current_slot(),
                                tick = tick_count,
                                "Accepting new attestations"
                            );
                            self.store.write().await.accept_new_attestations().await.expect("Failed to accept new attestations");
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
                        LeanChainServiceMessage::BuildAttestationData { slot, sender } => {
                            if let Err(err) = self.handle_build_attestation_data(slot, sender).await {
                                error!("Failed to handle build attestation data message: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::ProcessBlock { signed_block_with_attestation, is_trusted, need_gossip } => {
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

                            if let Err(err) = self.handle_process_block(*signed_block_with_attestation.clone(), is_trusted).await {
                                warn!("Failed to handle process block message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_gossip.send(LeanP2PRequest::GossipBlock(signed_block_with_attestation)) {
                                warn!("Failed to send item to outbound gossip channel: {err:?}");
                            }
                        }
                        LeanChainServiceMessage::ProcessAttestation { signed_attestation, is_trusted, need_gossip } => {
                            if enabled!(Level::DEBUG) {
                                debug!(
                                    slot = signed_attestation.message.slot(),
                                    head = ?signed_attestation.message.head(),
                                    source = ?signed_attestation.message.source(),
                                    target = ?signed_attestation.message.target(),
                                    "Processing attestation by Validator {}",
                                    signed_attestation.message.validator_id,
                                );
                            } else {
                                info!(
                                    slot = signed_attestation.message.slot(),
                                    source_slot = signed_attestation.message.source().slot,
                                    target_slot = signed_attestation.message.target().slot,
                                    "Processing attestation by Validator {}",
                                    signed_attestation.message.validator_id,
                                );
                            }

                            if let Err(err) = self.handle_process_attestation(*signed_attestation.clone(), is_trusted).await {
                                warn!("Failed to handle process block message: {err:?}");
                            }

                            if need_gossip && let Err(err) = self.outbound_gossip.send(LeanP2PRequest::GossipAttestation(signed_attestation)) {
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
        response: oneshot::Sender<BlockWithSignatures>,
    ) -> anyhow::Result<()> {
        let block_with_signatures = self
            .store
            .write()
            .await
            .produce_block_with_signatures(slot, slot % lean_network_spec().num_validators)
            .await?;

        // Send the produced block back to the requester
        response
            .send(block_with_signatures)
            .map_err(|err| anyhow!("Failed to send produced block: {err:?}"))?;

        Ok(())
    }

    async fn handle_build_attestation_data(
        &mut self,
        slot: u64,
        response: oneshot::Sender<AttestationData>,
    ) -> anyhow::Result<()> {
        let attestation_data = self.store.read().await.produce_attestation(slot).await?;

        // Send the built attestation data back to the requester
        response
            .send(attestation_data)
            .map_err(|err| anyhow!("Failed to send built attestation data: {err:?}"))?;

        Ok(())
    }

    async fn handle_process_block(
        &mut self,
        signed_block_with_attestation: SignedBlockWithAttestation,
        is_trusted: bool,
    ) -> anyhow::Result<()> {
        if !is_trusted {
            // TODO: Validate the signature.
        }

        self.store
            .write()
            .await
            .on_block(&signed_block_with_attestation.clone())
            .await?;

        Ok(())
    }

    async fn handle_process_attestation(
        &mut self,
        signed_attestation: SignedAttestation,
        is_trusted: bool,
    ) -> anyhow::Result<()> {
        if !is_trusted {
            // TODO: Validate the signature.
        }

        self.store
            .write()
            .await
            .on_attestation(signed_attestation, false)
            .await?;

        Ok(())
    }
}
