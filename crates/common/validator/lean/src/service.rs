use alloy_primitives::FixedBytes;
use anyhow::Context;
use ream_chain_lean::{
    clock::create_lean_clock_interval, lean_chain::LeanChainReader,
    messages::LeanChainServiceMessage,
};
use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};
use ream_network_spec::networks::lean_network_spec;
use tokio::sync::{mpsc, oneshot};
use tracing::info;
use tree_hash::TreeHash;

use crate::registry::LeanKeystore;

/// ValidatorService is responsible for managing validator operations
/// such as proposing blocks and voting on them. This service also holds the keystores
/// for its validators, which are used to sign.
///
/// Every first tick (t=0) it proposes a block if it's the validator's turn.
/// Every second tick (t=1/4) it votes on the proposed block.
///
/// NOTE: Other ticks should be handled by the other services, such as [LeanChainService].
pub struct ValidatorService {
    lean_chain: LeanChainReader,
    keystores: Vec<LeanKeystore>,
    chain_sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
}

impl ValidatorService {
    pub async fn new(
        lean_chain: LeanChainReader,
        keystores: Vec<LeanKeystore>,
        chain_sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
    ) -> Self {
        ValidatorService {
            lean_chain,
            keystores,
            chain_sender,
        }
    }

    pub async fn start(self) -> anyhow::Result<()> {
        info!(
            "ValidatorService started with {} validator(s), genesis_time: {}",
            self.keystores.len(),
            lean_network_spec().genesis_time
        );

        let mut tick_count = 0u64;

        // Start from slot 0, will be incremented for every slot boundary.
        let mut slot = 0;

        let mut interval =
            create_lean_clock_interval().context("Failed to create clock interval")?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match tick_count % 4 {
                        0 => {
                            slot += 1;

                            // First tick (t=0): Propose a block.
                            if let Some(keystore) = self.is_proposer(slot) {
                                info!("Validator {} proposing block for slot {slot} (tick {tick_count})", keystore.validator_id);

                                let (tx, rx) = oneshot::channel();
                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProduceBlock { slot, sender: tx })
                                    .expect("Failed to send vote to LeanChainService");

                                // Wait for the block to be produced.
                                let new_block = rx.await.expect("Failed to receive block from LeanChainService");

                                info!(
                                    "Validator {} built block: slot={}, root={:?}, parent={:?}, votes={}, state_root={:?}",
                                    keystore.validator_id,
                                    new_block.slot,
                                    new_block.tree_hash_root(),
                                    new_block.parent_root,
                                    new_block.body.attestations.len(),
                                    new_block.state_root
                                );

                                // TODO: Sign the block with the keystore.
                                let signed_block = SignedBlock {
                                    message: new_block,
                                    signature: FixedBytes::default(),
                                };

                                // Send block to the LeanChainService.
                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProcessBlock { signed_block, is_trusted: true, need_gossip: true })
                                    .expect("Failed to send block to LeanChainService");
                            } else {
                                let proposer_index = slot % lean_network_spec().num_validators;
                                info!("Not proposer for slot {slot} (proposer is validator {proposer_index}), skipping");
                            }
                        }
                        1 => {
                            // Second tick (t=1/4): Vote.
                            info!("Starting vote phase at slot {slot} (tick {tick_count}): {} validator(s) voting", self.keystores.len());

                            // Build the vote from LeanChain, and modify its validator ID
                            let vote_template = self.lean_chain.read().await.build_vote(slot).await.expect("Failed to build vote");
                            info!(
                                "Built vote template for head={:?}, slot={}, source={:?}, target={:?}",
                                vote_template.head,
                                vote_template.slot,
                                vote_template.source,
                                vote_template.target
                            );

                            // TODO: Sign the vote with the keystore.
                            let signed_votes = self.keystores.iter().map(|keystore| {
                                SignedVote {
                                    validator_id: keystore.validator_id,
                                    message: vote_template.clone(),
                                    signature: FixedBytes::default(),
                                }
                            }).collect::<Vec<_>>();

                            for signed_vote in signed_votes {
                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProcessVote { signed_vote, is_trusted: true, need_gossip: true })
                                    .expect("Failed to send vote to LeanChainService");
                            }
                        }
                        _ => {
                            // Other ticks (t=2/4, t=3/4): Do nothing.
                        }
                    }
                    tick_count += 1;
                }
            }
        }
    }

    /// Determine if one of the keystores is the proposer for the current slot.
    fn is_proposer(&self, slot: u64) -> Option<&LeanKeystore> {
        let proposer_index = slot % lean_network_spec().num_validators;

        self.keystores
            .iter()
            .find(|keystore| keystore.validator_id == proposer_index as u64)
    }
}
