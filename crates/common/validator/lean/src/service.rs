use anyhow::anyhow;
use ream_chain_lean::{
    clock::create_lean_clock_interval,
    lean_chain::LeanChainReader,
    messages::{LeanChainServiceMessage, QueueItem},
};
use ream_consensus_lean::VoteItem;
use ream_network_spec::networks::lean_network_spec;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

// TODO: We need to replace this after PQC integration.
// For now, we only need ID for keystore.
pub struct LeanKeystore {
    id: u64,
}

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
        // Hack: If no keystores are provided, create a default one.
        let keystores = if keystores.is_empty() {
            vec![
                LeanKeystore { id: 0 },
                LeanKeystore { id: 1 },
                LeanKeystore { id: 2 },
                LeanKeystore { id: 3 },
            ] // Placeholder for keystores
        } else {
            keystores
        };

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

        // Start from slot 1, will be incremented for every slot boundary.
        let mut slot = 1;

        let mut interval = create_lean_clock_interval()
            .map_err(|err| anyhow!("Failed to create clock interval: {err}"))?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match tick_count % 4 {
                        0 => {
                            slot += 1;

                            // First tick (t=0): Propose a block.
                            if let Some(keystore) = self.is_proposer(slot) {
                                info!("Validator {} proposing block for slot {slot} (tick {tick_count})", keystore.id);

                                let (tx, rx) = oneshot::channel();
                                self.chain_sender
                                    .send(LeanChainServiceMessage::produce_block(slot, tx))
                                    .expect("Failed to send vote to LeanChainService");


                                // Wait for the block to be produced.
                                let new_block = rx.await.expect("Failed to receive block from LeanChainService");

                                info!(
                                    "Validator {} built block: slot={}, parent={:?}, votes={}, state_root={:?}",
                                    keystore.id,
                                    new_block.slot,
                                    new_block.parent,
                                    new_block.votes.len(),
                                    new_block.state_root
                                );

                                // TODO 1: Sign the block with the keystore.

                                // Send block to the LeanChainService.
                                self.chain_sender
                                    .send(LeanChainServiceMessage::QueueItem(QueueItem::Block(new_block)))
                                    .expect("Failed to send vote to LeanChainService");
                            } else {
                                let proposer_index = slot % lean_network_spec().num_validators;
                                info!("Not proposer for slot {slot} (proposer is validator {proposer_index}), skipping");
                            }
                        }
                        1 => {
                            // Second tick (t=1/4): Vote.
                            info!("Starting vote phase at slot {slot} (tick {tick_count}): {} validator(s) voting", self.keystores.len());

                            // Build the vote from LeanChain, and modify its validator ID
                            let vote_template = self.lean_chain.read().await.build_vote().expect("Failed to build vote");

                            info!("Built vote template for head {:?} at slot {} with target {:?}", vote_template.head, vote_template.slot, vote_template.target.slot);

                            let votes = self.keystores.iter().map(|keystore| {
                                let mut vote = vote_template.clone();
                                vote.validator_id = keystore.id;
                                vote
                            }).collect::<Vec<_>>();

                            // TODO 1: Sign the votes with the keystore.
                            for vote in votes {
                                self.chain_sender
                                    .send(LeanChainServiceMessage::QueueItem(QueueItem::Vote(VoteItem::Unsigned(vote))))
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
            .find(|keystore| keystore.id == proposer_index as u64)
    }
}
