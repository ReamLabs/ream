use anyhow::anyhow;
use ream_chain_lean::{clock::create_lean_clock_interval, messages::LeanChainServiceMessage};
use ream_consensus_lean::{
    attestation::{Attestation, SignedAttestation},
    block::{BlockWithAttestation, BlockWithSignatures, SignedBlockWithAttestation},
};
use ream_keystore::lean_keystore::ValidatorKeystore;
use ream_network_spec::networks::lean_network_spec;
use tokio::sync::{mpsc, oneshot};
use tracing::{Level, debug, enabled, info};
use tree_hash::TreeHash;

/// ValidatorService is responsible for managing validator operations
/// such as proposing blocks and submitting attestations on them. This service also holds the
/// keystores for its validators, which are used to sign.
///
/// Every first tick (t=0) it proposes a block if it's the validator's turn.
/// Every second tick (t=1/4) it attestations on the proposed block.
///
/// NOTE: Other ticks should be handled by the other services, such as [LeanChainService].
pub struct ValidatorService {
    keystores: Vec<ValidatorKeystore>,
    chain_sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
}

impl ValidatorService {
    pub async fn new(
        keystores: Vec<ValidatorKeystore>,
        chain_sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
    ) -> Self {
        ValidatorService {
            keystores,
            chain_sender,
        }
    }

    pub async fn start(self) -> anyhow::Result<()> {
        info!(
            genesis_time = lean_network_spec().genesis_time,
            "ValidatorService started with {} validator(s)",
            self.keystores.len()
        );

        let mut tick_count = 0u64;

        let mut interval = create_lean_clock_interval()
            .map_err(|err| anyhow!("Failed to create clock interval: {err:?}"))?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let slot = tick_count / 4;
                    match tick_count % 4 {
                        0 => {
                            // First tick (t=0): Propose a block.
                            if slot > 0 && let Some(keystore) = self.is_proposer(slot) {
                                info!(slot, tick = tick_count, "Proposing block by Validator {}", keystore.index);
                                let (tx, rx) = oneshot::channel();

                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProduceBlock { slot, sender: tx })
                                    .expect("Failed to send produce block to LeanChainService");

                                // Wait for the block to be produced.
                                let BlockWithSignatures { block, mut signatures } = rx.await.expect("Failed to receive block from LeanChainService");

                                info!(
                                    slot = block.slot,
                                    block_root = ?block.tree_hash_root(),
                                    "Building block finished by Validator {}",
                                    keystore.index,
                                );

                            let (tx, rx) = oneshot::channel();
                            self.chain_sender
                                .send(LeanChainServiceMessage::BuildAttestationData { slot, sender: tx })
                                .expect("Failed to send attestation to LeanChainService");

                            let attestation_data = rx.await.expect("Failed to receive attestation data from LeanChainService");
                                let message = Attestation { validator_id: keystore.index, data: attestation_data };
                                signatures.push(keystore.private_key.sign(&message.tree_hash_root(), slot as u32)?).map_err(|err| anyhow!("Failed to push signature {err:?}"))?;
                                let signed_block_with_attestation = SignedBlockWithAttestation {
                                    message: BlockWithAttestation {
                                        block: block.clone(),
                                        proposer_attestation: message,
                                    },
                                    signature: signatures,
                                };

                                // Send block to the LeanChainService.
                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProcessBlock { signed_block_with_attestation: Box::new(signed_block_with_attestation), need_gossip: true })
                                    .map_err(|err| anyhow!("Failed to send block to LeanChainService: {err:?}"))?;
                            } else {

                                let proposer_index = slot % lean_network_spec().num_validators;
                                info!("Not proposer for slot {slot} (proposer is validator {proposer_index}), skipping");

                            }
                        }
                        1 => {
                            // Second tick (t=1/4): Attestation.
                            info!(slot, tick = tick_count, "Starting attestation phase: {} validator(s) voting", self.keystores.len());

                            let (tx, rx) = oneshot::channel();
                            self.chain_sender
                                .send(LeanChainServiceMessage::BuildAttestationData { slot, sender: tx })
                                .expect("Failed to send attestation to LeanChainService");

                            let attestation_data = rx.await.expect("Failed to receive attestation data from LeanChainService");

                            if enabled!(Level::DEBUG) {
                                debug!(
                                    slot = attestation_data.slot,
                                    head = ?attestation_data.head,
                                    source = ?attestation_data.source,
                                    target = ?attestation_data.target,
                                    "Building attestation data finished",
                                );
                            } else {
                                info!(
                                    slot = attestation_data.slot,
                                    head_slot = attestation_data.head.slot,
                                    source_slot = attestation_data.source.slot,
                                    target_slot = attestation_data.target.slot,
                                    "Building attestation data finished",
                                );
                            }

                            // TODO: Sign the attestation with the keystore.
                            let mut signed_attestations = vec![];
                            for (_, keystore) in self.keystores.iter().enumerate().filter(|(index, _)| *index as u64 != slot % lean_network_spec().num_validators) {
                                let message = Attestation {
                                        validator_id: keystore.index,
                                        data: attestation_data.clone()
                                    };
                                    signed_attestations.push(SignedAttestation {
                                    signature: keystore.private_key.sign(&message.tree_hash_root(), slot as u32)?,
                                    message,
                                });
                            }

                            for signed_attestation in signed_attestations {
                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProcessAttestation { signed_attestation: Box::new(signed_attestation), need_gossip: true })
                                    .map_err(|err| anyhow!("Failed to send attestation to LeanChainService: {err:?}"))?;
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
    fn is_proposer(&self, slot: u64) -> Option<&ValidatorKeystore> {
        let proposer_index = slot % lean_network_spec().num_validators;

        self.keystores
            .iter()
            .find(|keystore| keystore.index == proposer_index as u64)
    }
}
