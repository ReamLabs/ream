use anyhow::anyhow;
use ream_chain_lean::{
    clock::{create_lean_clock_interval, get_initial_tick_count},
    messages::{LeanChainServiceMessage, ServiceResponse},
};
use ream_consensus_lean::{
    attestation::{AggregatedAttestations, SignedAttestation},
    block::{
        BlockSignatures, BlockWithAttestation, BlockWithSignatures, SignedBlockWithAttestation,
    },
};
#[cfg(feature = "devnet3")]
use ream_consensus_misc::constants::lean::ATTESTATION_COMMITTEE_COUNT;
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
#[cfg(feature = "devnet3")]
use ream_fork_choice_lean::store::compute_subnet_id;
use ream_keystore::lean_keystore::ValidatorKeystore;
use ream_metrics::{
    PQ_SIGNATURE_ATTESTATION_SIGNING_TIME, VALIDATORS_COUNT, set_int_gauge_vec, start_timer,
    stop_timer,
};
use ream_network_spec::networks::lean_network_spec;
use tokio::sync::{mpsc, oneshot};
use tracing::{Level, debug, enabled, info, warn};
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
        set_int_gauge_vec(&VALIDATORS_COUNT, self.keystores.len() as i64, &[]);

        let mut tick_count = get_initial_tick_count();

        info!("ValidatorService starting at tick_count: {tick_count}");

        let mut interval = create_lean_clock_interval()
            .map_err(|err| anyhow!("Expected Ream to be started before genesis time: {err:?}"))?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let slot = tick_count / INTERVALS_PER_SLOT;
                    match tick_count % INTERVALS_PER_SLOT {
                        0 => {
                            // First tick (t=0): Propose a block.
                            if slot > 0 && let Some(keystore) = self.is_proposer(slot) {
                                info!(slot, tick = tick_count, "Proposing block by Validator {}", keystore.index);
                                let (tx, rx) = oneshot::channel();

                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProduceBlock { slot, sender: tx })
                                    .expect("Failed to send produce block to LeanChainService");

                                // Wait for the block to be produced.
                                let BlockWithSignatures { block, signatures } = match rx.await {
                                    Ok(ServiceResponse::Ok(block_with_signatures)) => block_with_signatures,
                                    Ok(ServiceResponse::Syncing) => {
                                        warn!("LeanChainService is syncing, cannot produce block for slot {slot}");
                                        tick_count += 1;
                                        continue;
                                    }
                                    Err(err) => {
                                        return Err(anyhow!("Failed to receive block from LeanChainService: {err:?}"));
                                    }
                                };

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

                                let attestation_data = match rx.await {
                                    Ok(ServiceResponse::Ok(data)) => data,
                                    Ok(ServiceResponse::Syncing) => {
                                        warn!("LeanChainService is syncing, cannot build attestation data for slot {slot}");
                                        tick_count += 1;
                                        continue;
                                    }
                                    Err(err) => {
                                        return Err(anyhow!("Failed to receive attestation data from LeanChainService: {err:?}"));
                                    }
                                };
                                let message = AggregatedAttestations { validator_id: keystore.index, data: attestation_data.clone() };

                                let timer = start_timer(&PQ_SIGNATURE_ATTESTATION_SIGNING_TIME, &[]);
                                let proposer_signature = keystore.private_key.sign(&attestation_data.tree_hash_root(), slot as u32)?;
                                stop_timer(timer);


                                let signed_block_with_attestation = SignedBlockWithAttestation {
                                    message: BlockWithAttestation {
                                        block: block.clone(),
                                        proposer_attestation: message,
                                    },
                                    signature: BlockSignatures {
                                        attestation_signatures: signatures,
                                        proposer_signature,
                                    },
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

                            let attestation_data = match rx.await {
                                Ok(ServiceResponse::Ok(data)) => data,
                                Ok(ServiceResponse::Syncing) => {
                                    warn!("LeanChainService is syncing, cannot build attestation data for slot {slot}");
                                    tick_count += 1;
                                    continue;
                                }
                                Err(err) => {
                                    return Err(anyhow!("Failed to receive attestation data from LeanChainService: {err:?}"));
                                }
                            };

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
                                let message = attestation_data.clone();
                                let timer = start_timer(&PQ_SIGNATURE_ATTESTATION_SIGNING_TIME, &[]);
                                let signature = keystore.private_key.sign(&message.tree_hash_root(), slot as u32)?;
                                stop_timer(timer);
                                signed_attestations.push(SignedAttestation {
                                    signature,
                                    message,
                                    validator_id: keystore.index,
                                });
                            }

                            for signed_attestation in signed_attestations {
                                #[cfg(feature = "devnet2")]
                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProcessAttestation { signed_attestation: Box::new(signed_attestation), need_gossip: true })
                                    .map_err(|err| anyhow!("Failed to send attestation to LeanChainService: {err:?}"))?;
                                #[cfg(feature = "devnet3")]
                                {
                                    let subnet_id = compute_subnet_id(signed_attestation.validator_id, ATTESTATION_COMMITTEE_COUNT);
                                    self.chain_sender
                                        .send(LeanChainServiceMessage::ProcessAttestation { signed_attestation: Box::new(signed_attestation), subnet_id, need_gossip: true })
                                        .map_err(|err| anyhow!("Failed to send attestation to LeanChainService: {err:?}"))?;
                                }
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
