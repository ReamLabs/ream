use std::{
    collections::HashSet,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use ream_chain_lean::{
    clock::{create_lean_clock_interval, get_initial_tick_count},
    messages::{LeanChainServiceMessage, ServiceResponse},
};
#[cfg(feature = "devnet5")]
use ream_consensus_lean::attestation::MultiMessageAggregate;
use ream_consensus_lean::{
    attestation::SignedAttestation,
    block::{BlockWithSignatures, SignedBlock},
};
use ream_consensus_misc::constants::lean::{INTERVALS_PER_SLOT, attestation_committee_count};
use ream_fork_choice_lean::store::compute_subnet_id;
use ream_metrics::{
    ATTESTATIONS_PRODUCTION_TIME, LEAN_ATTESTATION_AGGREGATE_SUBNETS,
    LEAN_ATTESTATION_AGGREGATE_VALIDATORS, PQ_SIG_ATTESTATION_SIGNATURES_TOTAL,
    PQ_SIG_ATTESTATION_SIGNING_TIME, VALIDATORS_COUNT, inc_int_counter_vec, set_int_gauge_vec,
    start_timer, stop_timer,
};
use ream_network_spec::networks::lean_network_spec;
#[cfg(feature = "devnet5")]
use ream_post_quantum_crypto::lean_multisig::type_2::{
    type_1_aggregate, type_1_from_wire, type_2_merge, type_2_to_wire,
};
#[cfg(feature = "devnet5")]
use ssz_types::VariableList;
use tokio::{
    sync::{mpsc, oneshot},
    time::{Duration, sleep},
};
use tracing::{Level, debug, enabled, info, warn};
use tree_hash::TreeHash;

use crate::signer::ProposalSigner;

/// ValidatorService is responsible for managing validator operations
/// such as proposing blocks and submitting attestations on them. This service also holds the
/// keystores for its validators, which are used to sign.
/// Every first tick (t=0) it proposes a block if it's the validator's turn.
/// Every second tick (t=1/4) it attestations on the proposed block.
/// NOTE: Other ticks should be handled by the other services, such as [LeanChainService].
pub struct ValidatorService {
    proposal_signer: Arc<ProposalSigner>,
    chain_sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
    prebuilding_slot: Option<u64>,
}

impl ValidatorService {
    pub async fn new(
        proposal_signer: Arc<ProposalSigner>,
        chain_sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
    ) -> Self {
        ValidatorService {
            proposal_signer,
            chain_sender,
            prebuilding_slot: None,
        }
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        info!(
            genesis_time = lean_network_spec().genesis_time,
            "ValidatorService started with {} validator(s)",
            self.proposal_signer.keystores().len()
        );
        set_int_gauge_vec(
            &VALIDATORS_COUNT,
            self.proposal_signer.keystores().len() as i64,
            &[],
        );

        let mut tick_count = get_initial_tick_count();

        info!("ValidatorService starting at tick_count: {tick_count}");

        let mut interval = create_lean_clock_interval()
            .map_err(|err| anyhow!("Expected Ream to be started before genesis time: {err:?}"))?;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let slot = tick_count / INTERVALS_PER_SLOT;
                    match tick_count % INTERVALS_PER_SLOT {
                        4 => {
                            let next_slot = slot + 1;
                            if let Some(validator_index) = self.proposer_index(next_slot) {
                                info!(slot = next_slot, tick = tick_count, "Pre-building block by Validator {validator_index}");
                                self.prebuilding_slot = Some(next_slot);
                                let chain_sender = self.chain_sender.clone();
                                let proposal_signer = self.proposal_signer.clone();
                                tokio::spawn(async move {
                                    build_block(chain_sender, next_slot, validator_index, proposal_signer).await;
                                });
                            }
                        }
                        0 => {
                            if self.prebuilding_slot == Some(slot) {
                                self.prebuilding_slot = None;
                            } else if slot > 0 && let Some(validator_index) = self.proposer_index(slot) {
                                info!(slot, tick = tick_count, "Proposing block by Validator {validator_index}");
                                let chain_sender = self.chain_sender.clone();
                                let proposal_signer = self.proposal_signer.clone();
                                tokio::spawn(async move {
                                    build_block(chain_sender, slot, validator_index, proposal_signer).await;
                                });
                            } else {
                                let proposer_index = slot % lean_network_spec().num_validators;
                                info!("Not proposer for slot {slot} (proposer is validator {proposer_index}), skipping");
                            }
                        }
                        1 => {
                            // Second tick (t=1/4): Attestation.
                            info!(slot, tick = tick_count, "Starting attestation phase: {} validator(s) voting", self.proposal_signer.keystores().len());

                            let attestation_production_timer =
                                start_timer(&ATTESTATIONS_PRODUCTION_TIME, &[]);
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
                                Ok(ServiceResponse::SyncLag { head_slot, lag, max_seen_slot }) => {
                                    warn!(
                                        slot,
                                        head_slot,
                                        lag,
                                        max_seen_slot,
                                        "Skipping attestation: validator is too far behind the chain head",
                                    );
                                    tick_count += 1;
                                    continue;
                                }
                                Ok(ServiceResponse::Err(err)) => {
                                    warn!("Failed to build attestation data for slot {slot}: {err}");
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

                            let message_root = attestation_data.tree_hash_root();
                            let mut signed_attestations =
                                Vec::with_capacity(self.proposal_signer.keystores().len());
                            for keystore in self.proposal_signer.keystores() {
                                let timer = start_timer(&PQ_SIG_ATTESTATION_SIGNING_TIME, &[]);
                                let signature = keystore.attestation_private_key.sign(&message_root, slot as u32)?;
                                stop_timer(timer);
                                inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_TOTAL, &[]);
                                signed_attestations.push(SignedAttestation {
                                    signature,
                                    message: attestation_data.clone(),
                                    validator_id: keystore.index,
                                });
                            }

                            let section = "timely";
                            let mut unique_subnets = HashSet::new();
                            let signed_attestations_count = signed_attestations.len();

                            for signed_attestation in signed_attestations {
                                let subnet_id = compute_subnet_id(signed_attestation.validator_id, attestation_committee_count());
                                unique_subnets.insert(subnet_id);

                                self.chain_sender
                                    .send(LeanChainServiceMessage::ProcessAttestation {
                                        signed_attestation: Box::new(signed_attestation),
                                        subnet_id,
                                        need_gossip: true
                                    })
                                    .map_err(|err| anyhow!("Failed to send attestation to LeanChainService: {err:?}"))?;
                            }

                            set_int_gauge_vec(
                                &LEAN_ATTESTATION_AGGREGATE_VALIDATORS,
                                signed_attestations_count as i64,
                                &[section, "combined"]
                            );

                            set_int_gauge_vec(
                                &LEAN_ATTESTATION_AGGREGATE_SUBNETS,
                                unique_subnets.len() as i64,
                                &[section]
                            );

                            stop_timer(attestation_production_timer);
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
    fn proposer_index(&self, slot: u64) -> Option<u64> {
        let proposer_index = slot % lean_network_spec().num_validators;

        self.proposal_signer
            .has_validator(proposer_index)
            .then_some(proposer_index)
    }
}

pub async fn build_block(
    chain_sender: mpsc::UnboundedSender<LeanChainServiceMessage>,
    slot: u64,
    validator_index: u64,
    proposal_signer: Arc<ProposalSigner>,
) {
    let (tx, rx) = oneshot::channel();

    if chain_sender
        .send(LeanChainServiceMessage::ProduceBlock { slot, sender: tx })
        .is_err()
    {
        warn!("Failed to send produce block to LeanChainService for slot {slot}");
        return;
    }

    // Wait for the block to be produced.
    let block_with_signatures = match rx.await {
        Ok(ServiceResponse::Ok(block_with_signatures)) => block_with_signatures,
        Ok(ServiceResponse::Syncing) => {
            warn!("LeanChainService is syncing, cannot produce block for slot {slot}");
            return;
        }
        Ok(ServiceResponse::SyncLag {
            head_slot,
            lag,
            max_seen_slot,
        }) => {
            warn!(
                slot,
                head_slot,
                lag,
                max_seen_slot,
                "Unreachable: Block production should not be gated by SyncLag",
            );
            return;
        }
        Ok(ServiceResponse::Err(err)) => {
            warn!("Failed to produce block for slot {slot}: {err}");
            return;
        }
        Err(err) => {
            warn!("Failed to receive block from LeanChainService for slot {slot}: {err:?}");
            return;
        }
    };

    info!(
        slot = block_with_signatures.block.slot,
        block_root = ?block_with_signatures.block.tree_hash_root(),
        "Building block finished by Validator {validator_index}",
    );

    let signed_block = match sign_block(
        &proposal_signer,
        validator_index,
        block_with_signatures,
        slot,
    ) {
        Ok(signed_block) => signed_block,
        Err(err) => {
            warn!("Failed to sign block for slot {slot}: {err}");
            return;
        }
    };

    let spec = lean_network_spec();
    let slot_start_ms = (spec.genesis_time + slot * spec.seconds_per_slot) * 1000;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(slot_start_ms);
    if now_ms < slot_start_ms {
        sleep(Duration::from_millis(slot_start_ms - now_ms)).await;
    }

    if let Err(err) = chain_sender.send(LeanChainServiceMessage::ProcessBlock {
        signed_block: Box::new(signed_block),
        need_gossip: true,
    }) {
        warn!("Failed to send block to LeanChainService for slot {slot}: {err:?}");
    }
}

#[cfg(feature = "devnet5")]
fn sign_block(
    proposal_signer: &ProposalSigner,
    validator_index: u64,
    block_with_signatures: BlockWithSignatures,
    slot: u64,
) -> anyhow::Result<SignedBlock> {
    let BlockWithSignatures {
        block,
        signatures,
        attestation_public_keys,
    } = block_with_signatures;

    if signatures.len() != attestation_public_keys.len() {
        return Err(anyhow!(
            "Attestation proof count ({}) does not match public-key-set count ({})",
            signatures.len(),
            attestation_public_keys.len()
        ));
    }

    let block_root = block.tree_hash_root();
    let block_root_bytes: [u8; 32] = block_root.into();

    let timer = start_timer(&PQ_SIG_ATTESTATION_SIGNING_TIME, &[]);
    let proposal_signature = proposal_signer.sign_proposal(validator_index, slot, &block_root)?;
    stop_timer(timer);
    inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_TOTAL, &[]);

    let mut components = Vec::with_capacity(signatures.len() + 1);
    for (proof, public_keys) in signatures.iter().zip(attestation_public_keys.iter()) {
        let component = type_1_from_wire(&proof.proof, public_keys).map_err(|err| {
            anyhow!("Failed to reconstruct attestation single-message aggregate proof: {err}")
        })?;
        components.push(component);
    }

    let proposer_type_1 = type_1_aggregate(
        &[],
        &[(proposal_signature.public_key, proposal_signature.signature)],
        &block_root_bytes,
        proposal_signature.epoch,
    )
    .map_err(|err| anyhow!("Failed to build proposer single-message aggregate proof: {err}"))?;
    components.push(proposer_type_1);

    let merged = type_2_merge(components)
        .map_err(|err| anyhow!("Failed to merge block multi-message aggregate proof: {err}"))?;

    Ok(SignedBlock {
        block,
        proof: MultiMessageAggregate::new(
            VariableList::new(type_2_to_wire(&merged))
                .map_err(|err| anyhow!("Block proof exceeds size limit: {err:?}"))?,
        ),
    })
}
