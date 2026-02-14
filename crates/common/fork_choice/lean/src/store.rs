use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
#[cfg(feature = "devnet3")]
use ream_consensus_lean::attestation::SignedAggregatedAttestation;
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof, AttestationData,
        SignatureKey, SignedAttestation,
    },
    block::{Block, BlockBody, BlockWithSignatures, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
    slot::is_justifiable_after,
    state::LeanState,
    validator::is_proposer,
};
#[cfg(feature = "devnet3")]
use ream_consensus_misc::constants::lean::ATTESTATION_COMMITTEE_COUNT;
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
#[cfg(feature = "devnet2")]
use ream_metrics::{
    ATTESTATION_VALIDATION_TIME, ATTESTATIONS_INVALID_TOTAL, ATTESTATIONS_VALID_TOTAL,
    FINALIZATIONS_TOTAL, FINALIZED_SLOT, FORK_CHOICE_BLOCK_PROCESSING_TIME, HEAD_SLOT,
    JUSTIFIED_SLOT, LATEST_FINALIZED_SLOT, LATEST_JUSTIFIED_SLOT, PROPOSE_BLOCK_TIME,
    SAFE_TARGET_SLOT, inc_int_counter_vec, set_int_gauge_vec, start_timer, stop_timer,
};
#[cfg(feature = "devnet3")]
use ream_metrics::{
    FINALIZATIONS_TOTAL, FINALIZED_SLOT, FORK_CHOICE_BLOCK_PROCESSING_TIME, HEAD_SLOT,
    JUSTIFIED_SLOT, LATEST_FINALIZED_SLOT, LATEST_JUSTIFIED_SLOT, PROPOSE_BLOCK_TIME,
    SAFE_TARGET_SLOT, inc_int_counter_vec, set_int_gauge_vec, start_timer, stop_timer,
};
use ream_network_spec::networks::lean_network_spec;
use ream_network_state_lean::NetworkState;
use ream_post_quantum_crypto::{
    lean_multisig::aggregate::aggregate_signatures, leansig::signature::Signature,
};
use ream_storage::{
    db::lean::LeanDB,
    tables::{
        field::REDBField,
        lean::{
            aggregated_payloads::AggregatedPayloadsTable, gossip_signatures::GossipSignaturesTable,
        },
        table::REDBTable,
    },
};
use ream_sync::rwlock::{Reader, Writer};
#[cfg(feature = "devnet3")]
use ssz::{Decode, Encode};
use ssz_types::{BitList, VariableList, typenum::U4096};
use tokio::sync::Mutex;
use tree_hash::TreeHash;

use crate::constants::JUSTIFICATION_LOOKBACK_SLOTS;

pub type LeanStoreWriter = Writer<Store>;
pub type LeanStoreReader = Reader<Store>;

/// [Store] represents the state that the Lean node should maintain.
#[derive(Debug, Clone)]
pub struct Store {
    pub store: Arc<Mutex<LeanDB>>,
    pub network_state: Arc<NetworkState>,
    #[cfg(feature = "devnet3")]
    pub validator_id: Option<u64>,
    #[cfg(feature = "devnet3")]
    pub attestation_data_by_root: HashMap<B256, AttestationData>,
    #[cfg(feature = "devnet3")]
    pub latest_new_aggregated_payloads: HashMap<SignatureKey, Vec<AggregatedSignatureProof>>,
    #[cfg(feature = "devnet3")]
    pub latest_known_aggregated_payloads: HashMap<SignatureKey, Vec<AggregatedSignatureProof>>,
}

impl Store {
    /// Initialize forkchoice store from an anchor state and anchor block.
    pub fn get_forkchoice_store(
        anchor_block: SignedBlockWithAttestation,
        anchor_state: LeanState,
        db: LeanDB,
        time: Option<u64>,
        #[cfg(feature = "devnet3")] validator_id: Option<u64>,
    ) -> anyhow::Result<Store> {
        ensure!(
            anchor_block.message.block.state_root == anchor_state.tree_hash_root(),
            "Anchor block state root must match anchor state hash"
        );
        let anchor_root = anchor_block.message.block.tree_hash_root();
        let anchor_slot = anchor_block.message.block.slot;
        #[cfg(feature = "devnet2")]
        let anchor_checkpoint = Checkpoint {
            root: anchor_root,
            slot: anchor_slot,
        };

        #[cfg(feature = "devnet3")]
        let justified_checkpoint = Checkpoint {
            root: anchor_root,
            slot: anchor_state.latest_justified.slot,
        };
        #[cfg(feature = "devnet3")]
        let finalized_checkpoint = Checkpoint {
            root: anchor_root,
            slot: anchor_state.latest_finalized.slot,
        };

        db.time_provider()
            .insert(time.unwrap_or(anchor_slot * lean_network_spec().seconds_per_slot))
            .expect("Failed to insert anchor slot");
        db.block_provider()
            .insert(anchor_root, anchor_block)
            .expect("Failed to insert genesis block");
        #[cfg(feature = "devnet3")]
        db.latest_finalized_provider()
            .insert(finalized_checkpoint)
            .expect("Failed to insert latest finalized checkpoint");
        #[cfg(feature = "devnet3")]
        db.latest_justified_provider()
            .insert(justified_checkpoint)
            .expect("Failed to insert latest justified checkpoint");
        #[cfg(feature = "devnet2")]
        db.latest_finalized_provider()
            .insert(anchor_checkpoint)
            .expect("Failed to insert latest finalized checkpoint");
        #[cfg(feature = "devnet2")]
        db.latest_justified_provider()
            .insert(anchor_checkpoint)
            .expect("Failed to insert latest justified checkpoint");
        db.state_provider()
            .insert(anchor_root, anchor_state)
            .expect("Failed to insert genesis state");
        db.head_provider()
            .insert(anchor_root)
            .expect("Failed to insert genesis block hash");
        db.safe_target_provider()
            .insert(anchor_root)
            .expect("Failed to insert genesis block hash");

        Ok(Store {
            store: Arc::new(Mutex::new(db)),
            #[cfg(feature = "devnet2")]
            network_state: Arc::new(NetworkState::new(anchor_checkpoint, anchor_checkpoint)),
            #[cfg(feature = "devnet3")]
            network_state: Arc::new(NetworkState::new(
                justified_checkpoint,
                finalized_checkpoint,
            )),
            #[cfg(feature = "devnet3")]
            validator_id,
            #[cfg(feature = "devnet3")]
            attestation_data_by_root: HashMap::new(),
            #[cfg(feature = "devnet3")]
            latest_new_aggregated_payloads: HashMap::new(),
            #[cfg(feature = "devnet3")]
            latest_known_aggregated_payloads: HashMap::new(),
        })
    }

    /// Use LMD GHOST to get the head, given a particular root (usually the
    /// latest known justified block). Returns the head root and slot.
    async fn compute_lmd_ghost_head(
        &self,
        attestations: impl Iterator<Item = anyhow::Result<SignedAttestation>>,
        provided_root: B256,
        min_score: u64,
    ) -> anyhow::Result<(B256, u64)> {
        let mut root = provided_root;

        let (slot_index_table, block_provider) = {
            let db = self.store.lock().await;
            (db.slot_index_provider(), db.block_provider())
        };

        // Start at genesis by default
        if root == B256::ZERO {
            root = slot_index_table
                .get_oldest_root()?
                .ok_or(anyhow!("No blocks found to calculate fork choice"))?;
        }

        let start_slot = block_provider
            .get(root)?
            .expect("Failed to get block for root")
            .message
            .block
            .slot;
        // For each block, count the number of votes for that block. A vote
        // for any descendant of a block also counts as a vote for that block
        let mut weights = HashMap::<B256, u64>::new();

        for attestation in attestations {
            let attestation = attestation?;
            let mut current_root = attestation.message.head.root;

            while let Some(block) = block_provider.get(current_root)? {
                let block = block.message.block;

                if block.slot <= start_slot {
                    break;
                }

                *weights.entry(current_root).or_insert(0) += 1;

                current_root = block.parent_root;
            }
        }

        // Identify the children of each block
        let children_map = block_provider.get_children_map(min_score, &weights)?;

        // Start at the root (latest justified hash or genesis) and repeatedly
        // choose the child with the most latest votes, tiebreaking by slot then hash
        let mut head = root;
        let mut head_slot = start_slot;

        while let Some(children) = children_map.get(&head) {
            (head, head_slot) = children
                .iter()
                .map(|child_hash| {
                    let vote_weight = *weights.get(child_hash).unwrap_or(&0);
                    let slot = block_provider
                        .get(*child_hash)
                        .ok()
                        .flatten()
                        .map(|block| block.message.block.slot)
                        .unwrap_or(0);
                    (*child_hash, slot, (vote_weight, slot, *child_hash))
                })
                .max_by_key(|(_, _, key)| *key)
                .map(|(hash, slot, _)| (hash, slot))
                .ok_or_else(|| anyhow!("No children found for current root: {head}"))?;
        }

        Ok((head, head_slot))
    }

    pub async fn get_block_id_by_slot(&self, slot: u64) -> anyhow::Result<B256> {
        self.store
            .lock()
            .await
            .slot_index_provider()
            .get(slot)?
            .ok_or_else(|| anyhow!("Block not found in chain for slot: {slot}"))
    }

    /// Compute the latest block that the validator is allowed to choose as the target
    /// and update as a safe target.
    pub async fn update_safe_target(&self) -> anyhow::Result<()> {
        // 2/3rd majority min voting weight for target selection
        // Note that we use ceiling division here.
        #[cfg(feature = "devnet2")]
        let (
            head_provider,
            state_provider,
            latest_justified_provider,
            safe_target_provider,
            latest_new_attestations_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.head_provider(),
                db.state_provider(),
                db.latest_justified_provider(),
                db.safe_target_provider(),
                db.latest_new_attestations_provider(),
            )
        };

        #[cfg(feature = "devnet3")]
        let (head_provider, state_provider, latest_justified_provider, safe_target_provider) = {
            let db = self.store.lock().await;
            (
                db.head_provider(),
                db.state_provider(),
                db.latest_justified_provider(),
                db.safe_target_provider(),
            )
        };

        let head_state = state_provider
            .get(head_provider.get()?)?
            .ok_or(anyhow!("Failed to get head state for safe target update"))?;

        let min_target_score = (head_state.validators.len() as u64 * 2).div_ceil(3);
        let latest_justified_root = latest_justified_provider.get()?.root;

        #[cfg(feature = "devnet3")]
        let attestations = self
            .extract_attestations_from_aggregated_payloads(&self.latest_new_aggregated_payloads)
            .await;

        let (new_safe_target_root, new_safe_target_slot) = self
            .compute_lmd_ghost_head(
                #[cfg(feature = "devnet2")]
                latest_new_attestations_provider.iter_values()?,
                #[cfg(feature = "devnet3")]
                attestations.into_iter().map(|(validator, data)| {
                    Ok(SignedAttestation {
                        validator_id: validator,
                        message: data,
                        signature: Signature::blank(),
                    })
                }),
                latest_justified_root,
                min_target_score,
            )
            .await?;

        safe_target_provider.insert(new_safe_target_root)?;

        // Update safe target slot metric
        set_int_gauge_vec(&SAFE_TARGET_SLOT, new_safe_target_slot as i64, &[]);

        Ok(())
    }

    #[cfg(feature = "devnet2")]
    pub async fn accept_new_attestations(&self) -> anyhow::Result<()> {
        let latest_known_attestation_provider = {
            let db = self.store.lock().await;
            db.latest_known_attestations_provider()
        };

        latest_known_attestation_provider.batch_insert(
            self.store
                .lock()
                .await
                .latest_new_attestations_provider()
                .drain()?
                .into_iter(),
        )?;

        self.update_head().await?;
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    pub async fn accept_new_attestations(&mut self) -> anyhow::Result<()> {
        let mut merged_aggregated_payloads = (self.latest_known_aggregated_payloads).clone();
        for (sig_key, mut proofs) in self.latest_new_aggregated_payloads.drain() {
            merged_aggregated_payloads
                .entry(sig_key)
                .or_default()
                .append(&mut proofs);
        }

        self.latest_known_aggregated_payloads = merged_aggregated_payloads;

        self.update_head().await?;

        Ok(())
    }

    #[cfg(feature = "devnet2")]
    pub async fn tick_interval(&self, has_proposal: bool) -> anyhow::Result<()> {
        let current_interval = {
            let time_provider = self.store.lock().await.time_provider();
            let time = time_provider.get()? + 1;
            time_provider.insert(time)?;
            time % lean_network_spec().seconds_per_slot % INTERVALS_PER_SLOT
        };
        if current_interval == 0 {
            if has_proposal {
                self.accept_new_attestations().await?;
            }
        } else if current_interval == 2 {
            self.update_safe_target().await?;
        } else if current_interval == 3 {
            self.accept_new_attestations().await?;
        };
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    pub async fn tick_interval(
        &mut self,
        has_proposal: bool,
        is_aggregator: bool,
    ) -> anyhow::Result<()> {
        let current_interval = {
            let time_provider = self.store.lock().await.time_provider();
            let time = time_provider.get()? + 1;
            time_provider.insert(time)?;
            time % INTERVALS_PER_SLOT
        };

        if current_interval == 0 {
            if has_proposal {
                self.accept_new_attestations().await?;
            }
        } else if current_interval == 2 {
            self.update_safe_target().await?;
            if is_aggregator {
                self.aggregate_committee_signatures().await?;
            }
        } else if current_interval == 3 {
            self.update_safe_target().await?;
        } else if current_interval == 4 {
            self.accept_new_attestations().await?;
        }

        Ok(())
    }

    #[cfg(feature = "devnet2")]
    pub async fn on_tick(&self, time: u64, has_proposal: bool) -> anyhow::Result<()> {
        let seconds_per_interval = lean_network_spec().seconds_per_slot / INTERVALS_PER_SLOT;
        let tick_interval_time = (time - lean_network_spec().genesis_time) / seconds_per_interval;

        let time_provider = self.store.lock().await.time_provider();
        while time_provider.get()? < tick_interval_time {
            let should_signal_proposal =
                has_proposal && (time_provider.get()? + 1) == tick_interval_time;

            self.tick_interval(should_signal_proposal).await?;
        }
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    pub async fn on_tick(
        &mut self,
        time: u64,
        has_proposal: bool,
        is_aggregator: bool,
    ) -> anyhow::Result<()> {
        let time_delta_ms = (time - lean_network_spec().genesis_time) * 1000;
        let tick_interval_time = time_delta_ms / lean_network_spec().seconds_per_slot * 1000 / 5;

        let time_provider = self.store.lock().await.time_provider();
        while time_provider.get()? < tick_interval_time {
            let should_signal_proposal =
                has_proposal && (time_provider.get()? + 1) == tick_interval_time;

            self.tick_interval(should_signal_proposal, is_aggregator)
                .await?;
        }
        Ok(())
    }

    /// Done upon processing new attestations or a new block
    pub async fn update_head(&self) -> anyhow::Result<()> {
        #[cfg(feature = "devnet2")]
        let latest_known_attestations = self
            .store
            .lock()
            .await
            .latest_known_attestations_provider()
            .get_all_attestations();
        let (latest_justified_provider, head_provider) = {
            let db = self.store.lock().await;
            (db.latest_justified_provider(), db.head_provider())
        };

        #[cfg(feature = "devnet3")]
        let attestations = self
            .extract_attestations_from_aggregated_payloads(&self.latest_known_aggregated_payloads)
            .await;

        let (new_head, new_head_slot) = self
            .compute_lmd_ghost_head(
                #[cfg(feature = "devnet2")]
                latest_known_attestations?.into_values().map(Ok),
                #[cfg(feature = "devnet3")]
                attestations.into_iter().map(|(vid, data)| {
                    Ok(SignedAttestation {
                        validator_id: vid,
                        message: data,
                        signature: Signature::blank(),
                    })
                }),
                latest_justified_provider.get()?.root,
                0,
            )
            .await?;

        set_int_gauge_vec(&HEAD_SLOT, new_head_slot as i64, &[]);
        *self.network_state.head_checkpoint.write() = Checkpoint {
            root: new_head,
            slot: new_head_slot,
        };
        head_provider.insert(new_head)?;

        Ok(())
    }

    pub async fn get_attestation_target(&self) -> anyhow::Result<Checkpoint> {
        let (head_provider, block_provider, safe_target_provider, latest_finalized_provider) = {
            let db = self.store.lock().await;
            (
                db.head_provider(),
                db.block_provider(),
                db.safe_target_provider(),
                db.latest_finalized_provider(),
            )
        };

        let mut target_block_root = head_provider.get()?;

        for _ in 0..JUSTIFICATION_LOOKBACK_SLOTS {
            if block_provider
                .get(target_block_root)?
                .ok_or(anyhow!("Block not found for target block root"))?
                .message
                .block
                .slot
                > block_provider
                    .get(safe_target_provider.get()?)?
                    .ok_or(anyhow!("Block not found for safe target"))?
                    .message
                    .block
                    .slot
            {
                target_block_root = block_provider
                    .get(target_block_root)?
                    .ok_or(anyhow!("Block not found for target block root"))?
                    .message
                    .block
                    .parent_root;
            } else {
                break;
            }
        }

        let latest_finalized_slot = latest_finalized_provider.get()?.slot;
        while !is_justifiable_after(
            block_provider
                .get(target_block_root)?
                .ok_or(anyhow!("Block not found for target block root"))?
                .message
                .block
                .slot,
            latest_finalized_slot,
        )? {
            target_block_root = block_provider
                .get(target_block_root)?
                .ok_or(anyhow!("Block not found for target block root"))?
                .message
                .block
                .parent_root;
        }

        let target_block = block_provider
            .get(target_block_root)?
            .ok_or(anyhow!("Block not found for target block root"))?;

        Ok(Checkpoint {
            root: target_block.message.block.tree_hash_root(),
            slot: target_block.message.block.slot,
        })
    }

    /// Get the head for block proposal at given slot.
    /// Ensures store is up-to-date and processes any pending attestations.
    #[cfg(feature = "devnet2")]
    pub async fn get_proposal_head(&self, slot: u64) -> anyhow::Result<B256> {
        let slot_time =
            lean_network_spec().genesis_time + slot * lean_network_spec().seconds_per_slot;
        self.on_tick(slot_time, true).await?;
        self.accept_new_attestations().await?;
        Ok(self.store.lock().await.head_provider().get()?)
    }

    /// Get the head for block proposal at given slot.
    /// Ensures store is up-to-date and processes any pending attestations.
    #[cfg(feature = "devnet3")]
    pub async fn get_proposal_head(&mut self, slot: u64) -> anyhow::Result<B256> {
        let slot_duration_seconds = slot * lean_network_spec().seconds_per_slot;
        let slot_time = lean_network_spec().genesis_time + slot_duration_seconds;
        self.on_tick(slot_time, true, false).await?;
        self.accept_new_attestations().await?;
        Ok(self.store.lock().await.head_provider().get()?)
    }

    /// Compute aggregated signatures for a set of attestations.
    ///
    /// This method implements a two-phase signature collection strategy:
    ///
    /// 1. **Gossip Phase**: For each attestation group, first attempt to collect individual XMSS
    ///    signatures from the gossip network.
    ///
    /// 2. **Fallback Phase**: For any validators not covered by gossip, fall back to
    ///    previously-seen aggregated proofs from blocks using a greedy set-cover approach to
    ///    minimize the number of proofs needed.
    #[cfg(feature = "devnet2")]
    fn compute_aggregated_signatures(
        &self,
        head_state: &LeanState,
        attestations: &[AggregatedAttestations],
        gossip_signatures_provider: &GossipSignaturesTable,
        aggregated_payloads_provider: &AggregatedPayloadsTable,
    ) -> anyhow::Result<(Vec<AggregatedAttestation>, Vec<AggregatedSignatureProof>)> {
        let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();
        for attestation in attestations.iter() {
            groups
                .entry(attestation.data.clone())
                .or_default()
                .push(attestation.validator_id);
        }

        let mut results = Vec::new();

        for (data, mut validator_ids) in groups {
            validator_ids.sort();
            let data_root = data.tree_hash_root();

            // Phase 1: Gossip Collection
            // Collect individual XMSS signatures from the gossip network
            let mut gossip_signatures = Vec::new();
            let mut gossip_keys = Vec::new();
            let mut gossip_ids = Vec::new();
            let mut remaining = HashSet::new();

            for &validator_id in &validator_ids {
                if let Ok(Some(signature)) = gossip_signatures_provider
                    .get(SignatureKey::from_parts(validator_id, data_root))
                {
                    gossip_signatures.push(signature);
                    if let Some(validator) = head_state.validators.get(validator_id as usize) {
                        gossip_keys.push(validator.public_key);
                    }
                    gossip_ids.push(validator_id);
                } else {
                    remaining.insert(validator_id);
                }
            }

            // If we collected gossip signatures, aggregate them into a proof
            if !gossip_ids.is_empty() && gossip_signatures.len() == gossip_keys.len() {
                let mut bits = BitList::<U4096>::with_capacity(
                    gossip_ids.iter().max().map_or(0, |&id| id as usize + 1),
                )
                .map_err(|err| anyhow!("BitList error: {err:?}"))?;

                for id in &gossip_ids {
                    bits.set(*id as usize, true)
                        .map_err(|err| anyhow!("BitList error: {err:?}"))?;
                }

                results.push((
                    AggregatedAttestation {
                        aggregation_bits: bits.clone(),
                        message: data.clone(),
                    },
                    AggregatedSignatureProof::new(
                        bits,
                        VariableList::new(aggregate_signatures(
                            &gossip_keys,
                            &gossip_signatures,
                            &data_root.0,
                            data.slot as u32,
                        )?)
                        .map_err(|err| anyhow!("Failed to create proof_data: {err:?}"))?,
                    ),
                ));
            }

            // Phase 2: Fallback to existing proofs using greedy set-cover
            while let Some(&target_id) = remaining.iter().next() {
                let candidates = match aggregated_payloads_provider
                    .get(SignatureKey::from_parts(target_id, data_root))
                {
                    Ok(Some(payloads)) => payloads.proofs.to_vec(),
                    _ => break,
                };

                if candidates.is_empty() {
                    break;
                }

                let mut best_proof = None;
                let mut best_covered = HashSet::new();

                for proof in &candidates {
                    let covered = proof
                        .to_validator_indices()
                        .into_iter()
                        .collect::<HashSet<u64>>();
                    let overlap = covered
                        .intersection(&remaining)
                        .copied()
                        .collect::<HashSet<u64>>();
                    if overlap.len() > best_covered.len() {
                        best_proof = Some(proof);
                        best_covered = overlap;
                    }
                }

                if best_covered.is_empty() {
                    break;
                }

                if let Some(proof) = best_proof {
                    results.push((
                        AggregatedAttestation {
                            aggregation_bits: proof.participants.clone(),
                            message: data.clone(),
                        },
                        proof.clone(),
                    ));
                    remaining = remaining.difference(&best_covered).copied().collect();
                }
            }
        }

        // Unzip results into parallel lists
        if results.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let (attestations, proofs): (Vec<_>, Vec<_>) = results.into_iter().unzip();
        Ok((attestations, proofs))
    }

    #[cfg(feature = "devnet3")]
    fn aggregate_gossip_signatures(
        &self,
        head_state: &LeanState,
        attestations: &[AggregatedAttestations],
        gossip_signatures_provider: &GossipSignaturesTable,
    ) -> anyhow::Result<(Vec<AggregatedAttestation>, Vec<AggregatedSignatureProof>)> {
        let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();
        for attestation in attestations.iter() {
            groups
                .entry(attestation.data.clone())
                .or_default()
                .push(attestation.validator_id);
        }

        let mut results = Vec::new();

        for (data, mut validator_ids) in groups {
            validator_ids.sort();
            let data_root = data.tree_hash_root();
            let mut gossip_signatures = Vec::new();
            let mut gossip_keys = Vec::new();
            let mut gossip_ids = Vec::new();

            for &validator_id in &validator_ids {
                if let Ok(Some(signature)) = gossip_signatures_provider
                    .get(SignatureKey::from_parts(validator_id, data_root))
                {
                    gossip_signatures.push(signature);
                    if let Some(validator) = head_state.validators.get(validator_id as usize) {
                        gossip_keys.push(validator.public_key);
                    }
                    gossip_ids.push(validator_id);
                }
            }

            if !gossip_ids.is_empty() && gossip_signatures.len() == gossip_keys.len() {
                let mut bits = BitList::<U4096>::with_capacity(
                    gossip_ids.iter().max().map_or(0, |&id| id as usize + 1),
                )
                .map_err(|err| anyhow!("BitList error: {err:?}"))?;

                for id in &gossip_ids {
                    bits.set(*id as usize, true)
                        .map_err(|err| anyhow!("BitList error: {err:?}"))?;
                }

                results.push((
                    AggregatedAttestation {
                        aggregation_bits: bits.clone(),
                        message: data.clone(),
                    },
                    AggregatedSignatureProof::new(
                        bits,
                        VariableList::new(aggregate_signatures(
                            &gossip_keys,
                            &gossip_signatures,
                            &data_root.0,
                            data.slot as u32,
                        )?)
                        .map_err(|err| anyhow!("Failed to create proof_data: {err:?}"))?,
                    ),
                ));
            }
        }

        if results.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let (attestations, proofs): (Vec<_>, Vec<_>) = results.into_iter().unzip();
        Ok((attestations, proofs))
    }

    #[cfg(feature = "devnet3")]
    fn select_aggregated_proofs(
        &self,
        attestations: &[AggregatedAttestations],
        aggregated_payloads_provider: &AggregatedPayloadsTable,
    ) -> anyhow::Result<(Vec<AggregatedAttestation>, Vec<AggregatedSignatureProof>)> {
        let mut results = Vec::new();
        let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();

        for attestation in attestations {
            groups
                .entry(attestation.data.clone())
                .or_default()
                .push(attestation.validator_id);
        }

        for (data, validator_ids) in groups {
            let data_root = data.tree_hash_root();
            let mut uncovered_indices: HashSet<u64> = validator_ids.into_iter().collect();

            while !uncovered_indices.is_empty() {
                let target_id = *uncovered_indices
                    .iter()
                    .next()
                    .expect("Failed to get target_id");

                let candidates = match aggregated_payloads_provider
                    .get(SignatureKey::from_parts(target_id, data_root))?
                {
                    Some(payloads) => payloads.proofs.to_vec(),
                    None => {
                        uncovered_indices.remove(&target_id);
                        continue;
                    }
                };

                let mut best_proof = None;
                let mut max_intersection = HashSet::new();

                for proof in &candidates {
                    let proof_indices: HashSet<u64> =
                        proof.to_validator_indices().into_iter().collect();
                    let intersection: HashSet<u64> = proof_indices
                        .intersection(&uncovered_indices)
                        .copied()
                        .collect();

                    if intersection.len() > max_intersection.len() {
                        max_intersection = intersection;
                        best_proof = Some(proof);
                    }
                }

                if let Some(proof) = best_proof {
                    results.push((
                        AggregatedAttestation {
                            aggregation_bits: proof.participants.clone(),
                            message: data.clone(),
                        },
                        proof.clone(),
                    ));

                    for id in max_intersection {
                        uncovered_indices.remove(&id);
                    }
                } else {
                    uncovered_indices.remove(&target_id);
                }
            }
        }

        let (attestations, proofs): (Vec<_>, Vec<_>) = results.into_iter().unzip();
        Ok((attestations, proofs))
    }

    pub async fn build_block(
        &self,
        slot: u64,
        proposer_index: u64,
        parent_root: B256,
        attestations: Option<VariableList<AggregatedAttestations, U4096>>,
    ) -> anyhow::Result<(Block, Vec<AggregatedSignatureProof>, LeanState)> {
        let (
            state_provider,
            latest_known_attestation_provider,
            block_provider,
            gossip_signatures_provider,
            aggregated_payloads_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.latest_known_attestations_provider(),
                db.block_provider(),
                db.gossip_signatures_provider(),
                db.aggregated_payloads_provider(),
            )
        };
        let available_signed_attestations =
            latest_known_attestation_provider.get_all_attestations()?;
        let head_state = state_provider
            .get(parent_root)?
            .ok_or(anyhow!("State not found for head root"))?;
        let mut attestations: VariableList<AggregatedAttestations, U4096> =
            attestations.unwrap_or_else(VariableList::empty);

        loop {
            let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();
            for attestation in attestations.iter() {
                groups
                    .entry(attestation.data.clone())
                    .or_default()
                    .push(attestation.validator_id);
            }

            let attestations_list = VariableList::new(
                groups
                    .into_iter()
                    .map(|(message, ids)| {
                        let mut bits = BitList::<U4096>::with_capacity(
                            ids.iter().max().map_or(0, |&id| id as usize + 1),
                        )
                        .map_err(|err| anyhow!("BitList error: {err:?}"))?;

                        for id in ids {
                            bits.set(id as usize, true)
                                .map_err(|err| anyhow!("BitList error: {err:?}"))?;
                        }
                        Ok(AggregatedAttestation {
                            aggregation_bits: bits,
                            message,
                        })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?,
            )
            .map_err(|err| anyhow!("Limit exceeded: {err:?}"))?;

            let candidate_block = Block {
                slot,
                proposer_index,
                parent_root,
                state_root: B256::ZERO,
                body: BlockBody {
                    attestations: attestations_list,
                },
            };
            let mut advanced_state = head_state.clone();
            advanced_state.process_slots(slot)?;
            advanced_state.process_block(&candidate_block)?;

            let mut new_attestations: VariableList<AggregatedAttestations, U4096> =
                VariableList::empty();

            for signed_attestation in available_signed_attestations.values() {
                let data = &signed_attestation.message;
                let validator_id = signed_attestation.validator_id;
                let data_root = data.tree_hash_root();
                let signature_key = SignatureKey::from_parts(validator_id, data_root);

                let attestation = AggregatedAttestations {
                    validator_id,
                    data: data.clone(),
                };

                if !block_provider.contains_key(data.head.root) {
                    continue;
                }

                if data.source != advanced_state.latest_justified {
                    continue;
                }

                if attestations.contains(&attestation) {
                    continue;
                }

                if gossip_signatures_provider
                    .get(signature_key.clone())
                    .ok()
                    .flatten()
                    .is_some()
                    || aggregated_payloads_provider
                        .get(signature_key.clone())
                        .ok()
                        .flatten()
                        .is_some()
                {
                    new_attestations
                        .push(attestation)
                        .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
                }
            }

            if new_attestations.is_empty() {
                break;
            }

            for attestation in new_attestations {
                attestations
                    .push(attestation)
                    .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
            }
        }

        let attestations_vec: Vec<_> = attestations.to_vec();

        #[cfg(feature = "devnet2")]
        let (aggregated_attestations, aggregated_proofs) = self.compute_aggregated_signatures(
            &head_state,
            &attestations_vec,
            &gossip_signatures_provider,
            &aggregated_payloads_provider,
        )?;

        #[cfg(feature = "devnet3")]
        let (aggregated_attestations, aggregated_proofs) =
            self.select_aggregated_proofs(&attestations_vec, &aggregated_payloads_provider)?;

        let attestations_list =
            VariableList::new(aggregated_attestations).map_err(|err| anyhow!("{err:?}"))?;

        let candidate_final_block = Block {
            slot,
            proposer_index,
            parent_root,
            state_root: B256::ZERO,
            body: BlockBody {
                attestations: attestations_list,
            },
        };

        let mut post_state = head_state.clone();
        post_state.process_slots(slot)?;
        post_state.process_block(&candidate_final_block)?;

        Ok((
            Block {
                slot,
                proposer_index,
                parent_root,
                state_root: post_state.tree_hash_root(),
                body: candidate_final_block.body,
            },
            aggregated_proofs,
            post_state,
        ))
    }

    pub async fn produce_block_with_signatures(
        &mut self,
        slot: u64,
        validator_index: u64,
    ) -> anyhow::Result<BlockWithSignatures> {
        let head_root = self.get_proposal_head(slot).await?;
        let initialize_block_timer = start_timer(&PROPOSE_BLOCK_TIME, &["initialize_block"]);
        let state_provider = self.store.lock().await.state_provider();

        let head_state = state_provider
            .get(head_root)?
            .ok_or(anyhow!("State not found for head root"))?;
        stop_timer(initialize_block_timer);

        let num_validators = head_state.validators.len();

        ensure!(
            is_proposer(validator_index, slot, num_validators as u64),
            "Validator {validator_index} is not the proposer for slot {slot}"
        );

        let add_attestations_timer =
            start_timer(&PROPOSE_BLOCK_TIME, &["add_valid_attestations_to_block"]);
        #[cfg(feature = "devnet2")]
        let (mut candidate_block, proofs, post_state) = self
            .build_block(slot, validator_index, head_root, None)
            .await?;

        #[cfg(feature = "devnet3")]
        let attestation_data_map = self
            .extract_attestations_from_aggregated_payloads(&self.latest_known_aggregated_payloads)
            .await;

        #[cfg(feature = "devnet3")]
        let attestation_vector: Vec<AggregatedAttestations> = attestation_data_map
            .into_iter()
            .map(|(vid, data)| AggregatedAttestations {
                validator_id: vid,
                data,
            })
            .collect();

        #[cfg(feature = "devnet3")]
        let attestation_list = VariableList::new(attestation_vector)
            .map_err(|err| anyhow!("Failed to create VariableList: {err:?}"))?;

        #[cfg(feature = "devnet3")]
        let (mut candidate_block, proofs, post_state) = self
            .build_block(slot, validator_index, head_root, Some(attestation_list))
            .await?;

        stop_timer(add_attestations_timer);

        let compute_state_root_timer = start_timer(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        candidate_block.state_root = post_state.tree_hash_root();
        stop_timer(compute_state_root_timer);

        let signatures_list = VariableList::new(proofs)
            .map_err(|err| anyhow!("Failed to return signatures {err:?}"))?;

        #[cfg(feature = "devnet3")]
        let latest_finalized_provider = self.store.lock().await.latest_finalized_provider();
        #[cfg(feature = "devnet3")]
        let finalized_advanced =
            post_state.latest_finalized.slot > latest_finalized_provider.get()?.slot;
        #[cfg(feature = "devnet3")]
        if finalized_advanced {
            self.prune_stale_attestation_data().await?;
        }

        Ok(BlockWithSignatures {
            block: candidate_block,
            signatures: signatures_list,
        })
    }

    pub async fn on_block(
        &mut self,
        signed_block_with_attestation: &SignedBlockWithAttestation,
        verify_signatures: bool,
    ) -> anyhow::Result<()> {
        let block_processing_timer = start_timer(&FORK_CHOICE_BLOCK_PROCESSING_TIME, &[]);

        let (state_provider, block_provider, latest_justified_provider, latest_finalized_provider) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.block_provider(),
                db.latest_justified_provider(),
                db.latest_finalized_provider(),
            )
        };
        let block = &signed_block_with_attestation.message.block;
        let proposer_attestation = &signed_block_with_attestation.message.proposer_attestation;
        let block_root = block.tree_hash_root();

        // If the block is already known, ignore it
        if block_provider.get(block_root)?.is_some() {
            stop_timer(block_processing_timer);
            return Ok(());
        }

        let mut parent_state = state_provider
            .get(block.parent_root)?
            .ok_or(anyhow!("State not found for parent root"))?;

        signed_block_with_attestation.verify_signatures(&parent_state, verify_signatures)?;
        parent_state.state_transition(block, true)?;

        let latest_justified =
            if parent_state.latest_justified.slot > latest_justified_provider.get()?.slot {
                parent_state.latest_justified
            } else {
                latest_justified_provider.get()?
            };

        #[cfg(feature = "devnet2")]
        let latest_finalized =
            if parent_state.latest_finalized.slot > latest_finalized_provider.get()?.slot {
                inc_int_counter_vec(&FINALIZATIONS_TOTAL, &["success"]);
                parent_state.latest_finalized
            } else {
                latest_finalized_provider.get()?
            };

        #[cfg(feature = "devnet3")]
        let finalized_advanced =
            parent_state.latest_finalized.slot > latest_finalized_provider.get()?.slot;
        #[cfg(feature = "devnet3")]
        let latest_finalized = if finalized_advanced {
            inc_int_counter_vec(&FINALIZATIONS_TOTAL, &["success"]);
            parent_state.latest_finalized
        } else {
            latest_finalized_provider.get()?
        };

        set_int_gauge_vec(&JUSTIFIED_SLOT, latest_justified.slot as i64, &[]);
        set_int_gauge_vec(&FINALIZED_SLOT, latest_finalized.slot as i64, &[]);
        set_int_gauge_vec(&LATEST_JUSTIFIED_SLOT, latest_justified.slot as i64, &[]);
        set_int_gauge_vec(&LATEST_FINALIZED_SLOT, latest_finalized.slot as i64, &[]);

        block_provider.insert(block_root, signed_block_with_attestation.clone())?;
        state_provider.insert(block_root, parent_state)?;
        latest_justified_provider.insert(latest_justified)?;
        latest_finalized_provider.insert(latest_finalized)?;
        *self.network_state.finalized_checkpoint.write() = latest_finalized;

        let aggregated_attestations = &signed_block_with_attestation
            .message
            .block
            .body
            .attestations;
        let attestation_signatures = &signed_block_with_attestation
            .signature
            .attestation_signatures;

        ensure!(
            aggregated_attestations.len() == attestation_signatures.len(),
            "Attestation signature groups must match aggregated attestations"
        );

        #[cfg(feature = "devnet2")]
        {
            let aggregated_payloads_provider =
                self.store.lock().await.aggregated_payloads_provider();

            // Process each aggregated attestation for fork choice and store proofs
            for (aggregated_attestation, aggregated_proof) in aggregated_attestations
                .iter()
                .zip(attestation_signatures.iter())
            {
                let validator_ids: Vec<u64> = aggregated_attestation
                    .aggregation_bits
                    .iter()
                    .enumerate()
                    .filter(|(_, bit)| *bit)
                    .map(|(index, _)| index as u64)
                    .collect();

                // Process each validator's attestation for fork choice and store proofs
                for validator_id in validator_ids {
                    // Store the aggregated proof with participants for this validator
                    aggregated_payloads_provider.append_proof(
                        SignatureKey::from_parts(
                            validator_id,
                            aggregated_attestation.message.tree_hash_root(),
                        ),
                        aggregated_proof.clone(),
                    )?;

                    self.on_gossip_aggregated_attestation(
                        SignedAttestation {
                            validator_id,
                            message: aggregated_attestation.message.clone(),
                            signature: Signature::blank(),
                        },
                        true,
                    )
                    .await?;
                }
            }
        }

        #[cfg(feature = "devnet3")]
        {
            for (attestation, proof) in aggregated_attestations
                .iter()
                .zip(attestation_signatures.iter())
            {
                let validator_ids = proof.to_validator_indices();
                let data_root = attestation.message.tree_hash_root();

                self.attestation_data_by_root
                    .insert(data_root, attestation.message.clone());

                for validator_id in validator_ids {
                    let key = SignatureKey::from_parts(validator_id, data_root);
                    self.latest_known_aggregated_payloads
                        .entry(key)
                        .or_default()
                        .push(proof.clone());
                }

                self.attestation_data_by_root.insert(
                    proposer_attestation.data.tree_hash_root(),
                    proposer_attestation.data.clone(),
                );
            }
        }

        self.update_head().await?;

        let gossip_signatures_provider = self.store.lock().await.gossip_signatures_provider();
        #[cfg(feature = "devnet2")]
        gossip_signatures_provider.insert(
            SignatureKey::new(
                proposer_attestation.validator_id,
                &proposer_attestation.data,
            ),
            signed_block_with_attestation
                .signature
                .proposer_signature
                .clone(),
        )?;

        #[cfg(feature = "devnet3")]
        let proposer_validator_id = proposer_attestation.validator_id;

        #[cfg(feature = "devnet3")]
        if let Some(current_id) = self.validator_id {
            let proposer_subnet =
                compute_subnet_id(proposer_validator_id, ATTESTATION_COMMITTEE_COUNT);
            let current_subnet = compute_subnet_id(current_id, ATTESTATION_COMMITTEE_COUNT);

            if proposer_subnet == current_subnet {
                gossip_signatures_provider.insert(
                    SignatureKey::new(
                        proposer_attestation.validator_id,
                        &proposer_attestation.data,
                    ),
                    signed_block_with_attestation
                        .signature
                        .proposer_signature
                        .clone(),
                )?;
            }
        }

        #[cfg(feature = "devnet2")]
        {
            self.on_gossip_aggregated_attestation(
                SignedAttestation {
                    validator_id: proposer_attestation.validator_id,
                    message: proposer_attestation.data.clone(),
                    signature: signed_block_with_attestation
                        .signature
                        .proposer_signature
                        .clone(),
                },
                false,
            )
            .await?;
        }

        #[cfg(feature = "devnet3")]
        {
            let mut participants = BitList::with_capacity(4096)
                .map_err(|err| anyhow!("Bitfield capacity error: {err:?}"))?;

            participants
                .set(proposer_validator_id as usize, true)
                .map_err(|err| anyhow!("Bitfield set error: {err:?}"))?;

            let signature_bytes = signed_block_with_attestation
                .signature
                .proposer_signature
                .as_ssz_bytes();

            let proof_data = signature_bytes.try_into().map_err(|err| {
                anyhow!("Failed to convert proposer signature to VariableList {err:?}")
            })?;

            self.on_gossip_aggregated_attestation(SignedAggregatedAttestation {
                data: proposer_attestation.data.clone(),
                proof: AggregatedSignatureProof {
                    participants,
                    proof_data,
                },
            })
            .await?;
        }

        #[cfg(feature = "devnet3")]
        if finalized_advanced {
            self.prune_stale_attestation_data().await?;
        }

        stop_timer(block_processing_timer);
        Ok(())
    }

    pub async fn validate_attestation(
        &self,
        signed_attestation: &SignedAttestation,
    ) -> anyhow::Result<()> {
        let data = &signed_attestation.message;
        let block_provider = self.store.lock().await.block_provider();

        // Validate attestation targets exist in store
        ensure!(
            block_provider.contains_key(data.source.root),
            "Unknown source block: {}",
            data.source.root
        );
        ensure!(
            block_provider.contains_key(data.target.root),
            "Unknown target block: {}",
            data.target.root
        );
        ensure!(
            block_provider.contains_key(data.head.root),
            "Unknown head block: {}",
            data.head.root
        );
        ensure!(
            data.source.slot <= data.target.slot,
            "Source checkpoint slot must not exceed target"
        );

        // Validate slot relationships
        let source_block = block_provider
            .get(data.source.root)?
            .ok_or(anyhow!("Failed to get source block"))?;

        let target_block = block_provider
            .get(data.target.root)?
            .ok_or(anyhow!("Failed to get target block"))?;
        ensure!(
            source_block.message.block.slot == data.source.slot,
            "Source checkpoint slot mismatch"
        );

        ensure!(
            target_block.message.block.slot == data.target.slot,
            "Target checkpoint slot mismatch"
        );

        let current_slot =
            self.store.lock().await.time_provider().get()? / lean_network_spec().seconds_per_slot;
        ensure!(
            data.slot <= current_slot + 1,
            "Attestation too far in future expected slot: {} <= {}",
            data.slot,
            current_slot + 1,
        );

        Ok(())
    }

    pub async fn on_gossip_aggregated_attestation(
        &mut self,
        #[cfg(feature = "devnet2")] signed_attestation: SignedAttestation,
        #[cfg(feature = "devnet2")] is_from_block: bool,
        #[cfg(feature = "devnet3")] signed_attestation: SignedAggregatedAttestation,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "devnet2")]
        let (latest_known_attestations_provider, latest_new_attestations_provider, time_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_known_attestations_provider(),
                db.latest_new_attestations_provider(),
                db.time_provider(),
            )
        };

        #[cfg(feature = "devnet3")]
        let time_provider = self.store.lock().await.time_provider();

        #[cfg(feature = "devnet2")]
        let validate_attestation_timer = start_timer(&ATTESTATION_VALIDATION_TIME, &[]);

        #[cfg(feature = "devnet2")]
        match self.validate_attestation(&signed_attestation).await {
            Ok(_) => {
                inc_int_counter_vec(&ATTESTATIONS_VALID_TOTAL, &[]);
                stop_timer(validate_attestation_timer);
            }
            Err(err) => {
                inc_int_counter_vec(&ATTESTATIONS_INVALID_TOTAL, &[]);
                stop_timer(validate_attestation_timer);
                return Err(err);
            }
        }

        #[cfg(feature = "devnet2")]
        {
            let validator_id = signed_attestation.validator_id;
            let attestation_slot = signed_attestation.message.slot;

            if is_from_block {
                let latest_known = match latest_known_attestations_provider.get(validator_id)? {
                    Some(known) => known.message.slot < attestation_slot,
                    None => true,
                };
                if latest_known {
                    latest_known_attestations_provider
                        .insert(validator_id, signed_attestation.clone())?;
                }
                let remove = match latest_new_attestations_provider.get(validator_id)? {
                    Some(new) => new.message.slot <= attestation_slot,
                    None => false,
                };
                if remove {
                    latest_new_attestations_provider.remove(validator_id)?;
                }
            } else {
                let time_slots = time_provider.get()? / lean_network_spec().seconds_per_slot;
                ensure!(attestation_slot <= time_slots, "Future slot");

                let latest_new = match latest_new_attestations_provider.get(validator_id)? {
                    Some(new) => new.message.slot < attestation_slot,
                    None => true,
                };
                if latest_new {
                    latest_new_attestations_provider.insert(validator_id, signed_attestation)?;
                }
            }
        }

        #[cfg(feature = "devnet3")]
        {
            let data = &signed_attestation.data;
            let proof = &signed_attestation.proof;

            let data_root = data.tree_hash_root();
            let validator_ids = proof.to_validator_indices();
            let attestation_slot = data.slot;

            let state = self
                .store
                .lock()
                .await
                .state_provider()
                .get(data.target.root)?
                .ok_or_else(|| anyhow!("No state available for target {}", data.target.root))?;

            let public_keys: Vec<_> = validator_ids
                .iter()
                .map(|&validator| {
                    state
                        .validators
                        .get(validator as usize)
                        .map(|validator| validator.public_key)
                        .ok_or_else(|| anyhow!("Validator {validator} not found in state"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            let sig = Signature::from_ssz_bytes(proof.proof_data.as_ref())
                .map_err(|err| anyhow!("Failed to decode signature: {err:?}"))?;

            for pubkey in &public_keys {
                let is_valid = sig.verify(pubkey, attestation_slot as u32, &data_root.0)?;
                ensure!(is_valid, "Signature verification failed for validator");
            }

            self.attestation_data_by_root
                .insert(data_root, data.clone());

            for &vid in &validator_ids {
                let key = SignatureKey::from_parts(vid, data_root);

                self.latest_new_aggregated_payloads
                    .entry(key)
                    .or_default()
                    .push(proof.clone());
            }

            let time_slots = time_provider.get()? / lean_network_spec().seconds_per_slot;
            ensure!(
                attestation_slot <= time_slots,
                "Attestation from future slot {attestation_slot} <= {time_slots}"
            );
        }

        Ok(())
    }

    #[cfg(feature = "devnet3")]
    pub async fn extract_attestations_from_aggregated_payloads(
        &self,
        aggregated_payloads: &HashMap<SignatureKey, Vec<AggregatedSignatureProof>>,
    ) -> HashMap<u64, AttestationData> {
        let mut attestations: HashMap<u64, AttestationData> = HashMap::new();

        for (sig_key, proofs) in aggregated_payloads {
            let data_root = sig_key.data_root;
            let attestation_data = match self.attestation_data_by_root.get(&data_root) {
                Some(data) => data,
                None => continue,
            };

            for proof in proofs {
                let validator_ids = proof.to_validator_indices();
                for vid in validator_ids {
                    let is_newer = attestations
                        .get(&vid)
                        .is_none_or(|existing| existing.slot < attestation_data.slot);

                    if is_newer {
                        attestations.insert(vid, attestation_data.clone());
                    }
                }
            }
        }
        attestations
    }

    #[cfg(feature = "devnet3")]
    pub async fn aggregate_committee_signatures(&mut self) -> anyhow::Result<()> {
        let (state_provider, gossip_signatures_provider, head_root) = {
            let database = self.store.lock().await;
            (
                database.state_provider(),
                database.gossip_signatures_provider(),
                database.head_provider().get()?,
            )
        };

        let head_state = state_provider
            .get(head_root)?
            .ok_or_else(|| anyhow!("Head state not found"))?;

        let mut attestation_list = Vec::new();

        for sig_key in gossip_signatures_provider.get_keys()? {
            if let Some(data) = self.attestation_data_by_root.get(&sig_key.data_root) {
                attestation_list.push(AggregatedAttestations {
                    validator_id: sig_key.validator_id,
                    data: data.clone(),
                });
            }
        }

        let (aggregated_results, proofs) = self.aggregate_gossip_signatures(
            &head_state,
            &attestation_list,
            &gossip_signatures_provider,
        )?;

        for (aggregated_attestation, aggregated_signature) in
            aggregated_results.into_iter().zip(proofs)
        {
            let data_root = aggregated_attestation.message.tree_hash_root();
            for vid in aggregated_signature.to_validator_indices() {
                let key = SignatureKey::from_parts(vid, data_root);
                self.latest_new_aggregated_payloads
                    .entry(key)
                    .or_default()
                    .push(aggregated_signature.clone());
            }
        }

        Ok(())
    }

    /// Process a signed attestation from gossip network.
    /// 1. Validates attestation structure
    /// 2. Verifies XMSS signature
    /// 3. Stores the signature in gossip_signatures for later block building
    /// 4. Calls on_attestation to process the attestation data
    pub async fn on_gossip_attestation(
        &mut self,
        signed_attestation: SignedAttestation,
        #[cfg(feature = "devnet3")] is_aggregator: bool,
    ) -> anyhow::Result<()> {
        let validator_id = signed_attestation.validator_id;
        let attestation_data = &signed_attestation.message;
        let signature = signed_attestation.signature.clone();

        self.validate_attestation(&signed_attestation).await?;

        let (state_provider, gossip_signatures_provider) = {
            let db = self.store.lock().await;
            (db.state_provider(), db.gossip_signatures_provider())
        };
        let key_state = state_provider
            .get(attestation_data.target.root)?
            .ok_or_else(|| anyhow!("No state available for signature verification"))?;

        ensure!(
            validator_id < key_state.validators.len() as u64,
            "Validator {validator_id} not found in state",
        );

        ensure!(
            signature.verify(
                &key_state.validators[validator_id as usize].public_key,
                attestation_data.slot as u32,
                &attestation_data.tree_hash_root(),
            )?,
            "Signature verification failed"
        );

        #[cfg(feature = "devnet3")]
        let data_root = attestation_data.tree_hash_root();

        #[cfg(feature = "devnet2")]
        {
            gossip_signatures_provider
                .insert(SignatureKey::new(validator_id, attestation_data), signature)?;

            self.on_gossip_aggregated_attestation(signed_attestation, false)
                .await?;
        }

        #[cfg(feature = "devnet3")]
        {
            if is_aggregator {
                let current_id = self
                    .validator_id
                    .ok_or_else(|| anyhow!("Current validator ID must be set for aggregation"))?;

                let current_validator_subnet =
                    compute_subnet_id(current_id, ATTESTATION_COMMITTEE_COUNT);
                let attester_subnet = compute_subnet_id(validator_id, ATTESTATION_COMMITTEE_COUNT);

                if current_validator_subnet == attester_subnet {
                    gossip_signatures_provider
                        .insert(SignatureKey::new(validator_id, attestation_data), signature)?;
                }
            }

            self.attestation_data_by_root
                .insert(data_root, attestation_data.clone());
        }

        Ok(())
    }

    pub async fn produce_attestation_data(&self, slot: u64) -> anyhow::Result<AttestationData> {
        let (head_provider, block_provider, latest_justified_provider) = {
            let db = self.store.lock().await;
            (
                db.head_provider(),
                db.block_provider(),
                db.latest_justified_provider(),
            )
        };

        let head_root = head_provider.get()?;
        Ok(AttestationData {
            slot,
            head: Checkpoint {
                root: head_root,
                slot: block_provider
                    .get(head_root)?
                    .ok_or(anyhow!("Failed to get head block"))?
                    .message
                    .block
                    .slot,
            },
            target: self.get_attestation_target().await?,
            source: latest_justified_provider.get()?,
        })
    }

    #[cfg(feature = "devnet3")]
    pub async fn prune_stale_attestation_data(&mut self) -> anyhow::Result<()> {
        let (latest_finalized_provider, gossip_signatures_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_finalized_provider(),
                db.gossip_signatures_provider(),
            )
        };

        let finalized_slot = latest_finalized_provider.get()?.slot;
        let stale_roots: HashSet<B256> = self
            .attestation_data_by_root
            .iter()
            .filter(|(_, data)| data.target.slot <= finalized_slot)
            .map(|(root, _)| *root)
            .collect();

        if stale_roots.is_empty() {
            return Ok(());
        }

        self.attestation_data_by_root
            .retain(|root, _| !stale_roots.contains(root));

        self.latest_new_aggregated_payloads
            .retain(|key, _| !stale_roots.contains(&key.data_root));

        self.latest_known_aggregated_payloads
            .retain(|key, _| !stale_roots.contains(&key.data_root));

        gossip_signatures_provider.retain(|key| !stale_roots.contains(&key.data_root))?;

        Ok(())
    }
}

pub fn compute_subnet_id(validator_id: u64, num_committees: u64) -> u64 {
    validator_id % num_committees
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{B256, FixedBytes};
    #[cfg(feature = "devnet3")]
    use anyhow::ensure;
    use ream_consensus_lean::{
        attestation::{
            AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof,
            AttestationData, SignatureKey, SignedAttestation,
        },
        block::{
            Block, BlockSignatures, BlockWithAttestation, BlockWithSignatures,
            SignedBlockWithAttestation,
        },
        checkpoint::Checkpoint,
        state::LeanState,
        utils::generate_default_validators,
        validator::{Validator, is_proposer},
    };
    use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
    use ream_network_spec::networks::{LeanNetworkSpec, lean_network_spec, set_lean_network_spec};
    use ream_post_quantum_crypto::leansig::{private_key::PrivateKey, signature::Signature};
    use ream_storage::{
        db::{ReamDB, lean::LeanDB},
        tables::{field::REDBField, table::REDBTable},
    };
    use ssz_types::{BitList, VariableList, typenum::U4096};
    use tempdir::TempDir;
    use tree_hash::TreeHash;

    use super::Store;
    use crate::genesis::setup_genesis;

    pub fn db_setup() -> LeanDB {
        let temp_dir = TempDir::new("lean_test").unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        let ream_db = ReamDB::new(temp_path).expect("unable to init Ream Database");
        ream_db.init_lean_db().unwrap()
    }

    pub async fn sample_store(no_of_validators: usize) -> (Store, LeanState) {
        set_lean_network_spec(LeanNetworkSpec::ephemery().into());
        let (genesis_block, genesis_state) = setup_genesis(
            lean_network_spec().genesis_time,
            generate_default_validators(no_of_validators),
        );

        let checkpoint = Checkpoint {
            slot: genesis_block.slot,
            root: genesis_block.tree_hash_root(),
        };
        let signed_genesis_block = build_signed_block_with_attestation(
            AttestationData {
                slot: genesis_block.slot,
                head: checkpoint,
                target: checkpoint,
                source: checkpoint,
            },
            genesis_block.clone(),
            VariableList::default(),
        );

        (
            Store::get_forkchoice_store(
                signed_genesis_block,
                genesis_state.clone(),
                db_setup(),
                Some(0),
                #[cfg(feature = "devnet3")]
                None,
            )
            .unwrap(),
            genesis_state,
        )
    }

    pub fn build_signed_block_with_attestation(
        attestation_data: AttestationData,
        block: Block,
        attestation_signatures: VariableList<AggregatedSignatureProof, U4096>,
    ) -> SignedBlockWithAttestation {
        SignedBlockWithAttestation {
            message: BlockWithAttestation {
                proposer_attestation: AggregatedAttestations {
                    validator_id: block.proposer_index,
                    data: attestation_data,
                },
                block,
            },
            signature: BlockSignatures {
                attestation_signatures,
                proposer_signature: Signature::blank(),
            },
        }
    }

    #[cfg(feature = "devnet3")]
    fn _make_attestation_data(slot: u64, target_slot: u64) -> AttestationData {
        let mut root = B256::ZERO;
        root[24..32].copy_from_slice(&target_slot.to_be_bytes());

        AttestationData {
            slot,
            head: Checkpoint {
                root,
                slot: target_slot,
            },
            target: Checkpoint {
                root,
                slot: target_slot,
            },
            source: Checkpoint {
                root: B256::ZERO,
                slot: 0,
            },
        }
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_prunes_entries_with_target_at_finalized() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;
        let attestation_data = _make_attestation_data(5, 5);
        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::new(1, &attestation_data);

        {
            store
                .attestation_data_by_root
                .insert(data_root, attestation_data);
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::repeat_byte(0xff),
                    slot: 5,
                })
                .unwrap();
            db.gossip_signatures_provider()
                .insert(sig_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(store.attestation_data_by_root.contains_key(&data_root));
        {
            let db = store.store.lock().await;
            ensure!(
                db.gossip_signatures_provider()
                    .get(sig_key.clone())
                    .unwrap()
                    .is_some()
            );
        }

        store.prune_stale_attestation_data().await?;

        ensure!(!store.attestation_data_by_root.contains_key(&data_root));
        let db = store.store.lock().await;
        ensure!(
            db.gossip_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_prunes_entries_with_target_before_finalized() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;
        let attestation_data = _make_attestation_data(3, 3);
        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::new(1, &attestation_data);

        {
            store
                .attestation_data_by_root
                .insert(data_root, attestation_data);
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::repeat_byte(0xff),
                    slot: 5,
                })
                .unwrap();
            db.gossip_signatures_provider()
                .insert(sig_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(store.attestation_data_by_root.contains_key(&data_root));
        store.prune_stale_attestation_data().await?;

        ensure!(!store.attestation_data_by_root.contains_key(&data_root));
        let db = store.store.lock().await;
        ensure!(
            db.gossip_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_keeps_entries_with_target_after_finalized() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;
        let attestation_data = _make_attestation_data(10, 10);
        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::new(1, &attestation_data);

        {
            store
                .attestation_data_by_root
                .insert(data_root, attestation_data.clone());
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::repeat_byte(0xff),
                    slot: 5,
                })
                .unwrap();
            db.gossip_signatures_provider()
                .insert(sig_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(store.attestation_data_by_root.contains_key(&data_root));
        store.prune_stale_attestation_data().await?;

        ensure!(store.attestation_data_by_root.contains_key(&data_root));
        ensure!(store.attestation_data_by_root.get(&data_root).unwrap() == &attestation_data);
        let db = store.store.lock().await;
        ensure!(
            db.gossip_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_some()
        );
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_prunes_related_structures_together() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;

        let stale_attestation = _make_attestation_data(3, 3);
        let stale_root = stale_attestation.tree_hash_root();
        let stale_key = SignatureKey::new(1, &stale_attestation);

        let fresh_attestation = _make_attestation_data(10, 10);
        let fresh_root = fresh_attestation.tree_hash_root();
        let fresh_key = SignatureKey::new(2, &fresh_attestation);

        let mock_proof = AggregatedSignatureProof::new(
            ssz_types::BitList::with_capacity(4096).unwrap(),
            ssz_types::VariableList::empty(),
        );

        {
            store
                .attestation_data_by_root
                .insert(stale_root, stale_attestation);
            store
                .attestation_data_by_root
                .insert(fresh_root, fresh_attestation);

            store
                .latest_new_aggregated_payloads
                .insert(stale_key.clone(), vec![mock_proof.clone()]);
            store
                .latest_new_aggregated_payloads
                .insert(fresh_key.clone(), vec![mock_proof.clone()]);

            store
                .latest_known_aggregated_payloads
                .insert(stale_key.clone(), vec![mock_proof.clone()]);
            store
                .latest_known_aggregated_payloads
                .insert(fresh_key.clone(), vec![mock_proof]);

            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();
            db.gossip_signatures_provider()
                .insert(stale_key.clone(), Signature::blank())
                .unwrap();
            db.gossip_signatures_provider()
                .insert(fresh_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(store.attestation_data_by_root.contains_key(&stale_root));
        ensure!(
            store
                .latest_new_aggregated_payloads
                .contains_key(&stale_key)
        );
        ensure!(
            store
                .latest_known_aggregated_payloads
                .contains_key(&stale_key)
        );

        store.prune_stale_attestation_data().await?;

        ensure!(!store.attestation_data_by_root.contains_key(&stale_root));
        ensure!(
            !store
                .latest_new_aggregated_payloads
                .contains_key(&stale_key)
        );
        ensure!(
            !store
                .latest_known_aggregated_payloads
                .contains_key(&stale_key)
        );

        ensure!(store.attestation_data_by_root.contains_key(&fresh_root));
        ensure!(
            store
                .latest_new_aggregated_payloads
                .contains_key(&fresh_key)
        );

        let db = store.store.lock().await;
        ensure!(
            db.gossip_signatures_provider()
                .get(stale_key)
                .unwrap()
                .is_none()
        );
        ensure!(
            db.gossip_signatures_provider()
                .get(fresh_key)
                .unwrap()
                .is_some()
        );
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_returns_self_when_nothing_to_prune() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;
        let fresh_attestation = _make_attestation_data(10, 10);
        let data_root = fresh_attestation.tree_hash_root();

        {
            store
                .attestation_data_by_root
                .insert(data_root, fresh_attestation);
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();
        }

        let initial_len = store.attestation_data_by_root.len();
        store.prune_stale_attestation_data().await?;

        ensure!(store.attestation_data_by_root.len() == initial_len);
        ensure!(store.attestation_data_by_root.contains_key(&data_root));
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_handles_empty_attestation_data() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;
        ensure!(
            store.attestation_data_by_root.is_empty(),
            "Store should start empty"
        );

        store.prune_stale_attestation_data().await?;

        ensure!(
            store.attestation_data_by_root.is_empty(),
            "Store should remain empty"
        );
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_prunes_multiple_validators_same_data_root() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;
        let stale_data = _make_attestation_data(3, 3);
        let data_root = stale_data.tree_hash_root();
        let sig_key_1 = SignatureKey::new(1, &stale_data);
        let sig_key_2 = SignatureKey::new(2, &stale_data);

        {
            store.attestation_data_by_root.insert(data_root, stale_data);
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();

            let gossip = db.gossip_signatures_provider();
            gossip
                .insert(sig_key_1.clone(), Signature::blank())
                .unwrap();
            gossip
                .insert(sig_key_2.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(store.attestation_data_by_root.contains_key(&data_root));
        store.prune_stale_attestation_data().await?;

        ensure!(!store.attestation_data_by_root.contains_key(&data_root));
        let db = store.store.lock().await;
        ensure!(
            db.gossip_signatures_provider()
                .get(sig_key_1)
                .unwrap()
                .is_none()
        );
        ensure!(
            db.gossip_signatures_provider()
                .get(sig_key_2)
                .unwrap()
                .is_none()
        );
        Ok(())
    }

    #[cfg(feature = "devnet3")]
    #[tokio::test]
    async fn test_mixed_stale_and_fresh_entries() -> anyhow::Result<()> {
        let (mut store, _) = sample_store(10).await;
        let mut roots = vec![];

        {
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();
            let gossip = db.gossip_signatures_provider();

            for i in 1..=10 {
                let data = _make_attestation_data(i, i);
                let root = data.tree_hash_root();
                let key = SignatureKey::new(i, &data);

                store.attestation_data_by_root.insert(root, data);
                gossip.insert(key, Signature::blank()).unwrap();
                roots.push(root);
            }
        }

        store.prune_stale_attestation_data().await?;

        for (i, root) in roots.iter().enumerate() {
            let slot = (i + 1) as u64;
            if slot <= 5 {
                ensure!(!store.attestation_data_by_root.contains_key(root));
            } else {
                ensure!(store.attestation_data_by_root.contains_key(root));
            }
        }
        Ok(())
    }

    // BLOCK PRODUCTION TESTS

    /// Test block production fails for unauthorized proposer.
    #[tokio::test]
    async fn test_produce_block_unauthorized_proposer() {
        let (mut store, _) = sample_store(10).await;
        let block_with_signature = store.produce_block_with_signatures(1, 2).await;
        assert!(block_with_signature.is_err());
    }

    /// Test block production includes available attestations
    /// This test generates real key pairs for validators to ensure signature aggregation works.
    #[tokio::test]
    async fn test_produce_block_with_attestations() {
        use rand::rng;

        // Generate real key pairs for validators
        let mut test_rng = rng();
        let mut validators = Vec::new();
        let mut private_keys = Vec::new();

        for i in 0..10 {
            let (pub_key, priv_key) = PrivateKey::generate_key_pair(&mut test_rng, 0, 10);
            validators.push(Validator {
                public_key: pub_key,
                index: i as u64,
            });
            private_keys.push(priv_key);
        }

        // Create store with validators that have real keys
        set_lean_network_spec(LeanNetworkSpec::ephemery().into());
        let (genesis_block, genesis_state) =
            crate::genesis::setup_genesis(lean_network_spec().genesis_time, validators);

        let checkpoint = Checkpoint {
            slot: genesis_block.slot,
            root: genesis_block.tree_hash_root(),
        };
        let signed_genesis_block = build_signed_block_with_attestation(
            AttestationData {
                slot: genesis_block.slot,
                head: checkpoint,
                target: checkpoint,
                source: checkpoint,
            },
            genesis_block.clone(),
            VariableList::default(),
        );

        let mut store = Store::get_forkchoice_store(
            signed_genesis_block,
            genesis_state.clone(),
            db_setup(),
            Some(0),
            #[cfg(feature = "devnet3")]
            None,
        )
        .unwrap();

        let (
            head_provider,
            block_provider,
            justified_provider,
            latest_known_attestations,
            gossip_signatures_provider,
        ) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.block_provider(),
                db.latest_justified_provider(),
                db.latest_known_attestations_provider(),
                db.gossip_signatures_provider(),
            )
        };
        let head = head_provider.get().unwrap();
        let head_block = block_provider.get(head).unwrap().unwrap();
        let justified_checkpoint = justified_provider.get().unwrap();
        let attestation_target = store.get_attestation_target().await.unwrap();

        // Create attestation data
        let attestation_data = AttestationData {
            slot: head_block.message.block.slot,
            head: Checkpoint {
                root: head,
                slot: head_block.message.block.slot,
            },
            target: justified_checkpoint,
            source: attestation_target,
        };

        // Sign attestation data with real private keys
        // The signature is over the attestation data tree hash root
        let data_root = attestation_data.tree_hash_root();
        let epoch = attestation_data.slot as u32;

        let signature_1 = private_keys[5].sign(&data_root.0, epoch).unwrap();
        let signature_2 = private_keys[6].sign(&data_root.0, epoch).unwrap();

        let attestation_1 = SignedAttestation {
            message: attestation_data.clone(),
            signature: signature_1,
            validator_id: 5,
        };

        let attestation_2 = SignedAttestation {
            message: attestation_data.clone(),
            signature: signature_2,
            validator_id: 6,
        };

        // Store attestations in latest_known_attestations
        latest_known_attestations
            .batch_insert([(5, attestation_1.clone()), (6, attestation_2.clone())])
            .unwrap();

        // Also store signatures in gossip_signatures
        // This is required for build_block to include the attestations
        gossip_signatures_provider
            .insert(
                SignatureKey::new(5, &attestation_1.message),
                attestation_1.signature,
            )
            .unwrap();
        gossip_signatures_provider
            .insert(
                SignatureKey::new(6, &attestation_2.message),
                attestation_2.signature,
            )
            .unwrap();

        let block_with_signature = store.produce_block_with_signatures(2, 2).await.unwrap();

        assert!(!block_with_signature.block.body.attestations.is_empty());
        assert_eq!(block_with_signature.block.slot, 2);
        assert_eq!(block_with_signature.block.proposer_index, 2);
        assert_eq!(
            block_with_signature.block.parent_root,
            store.get_proposal_head(2).await.unwrap()
        );
        assert_ne!(block_with_signature.block.state_root, B256::ZERO);
    }

    /// Test producing blocks in sequential slots.
    #[tokio::test]
    pub async fn test_produce_block_sequential_slots() {
        let (mut store, mut genesis_state) = sample_store(10).await;
        let block_provider = store.store.lock().await.block_provider();

        genesis_state.process_slots(1).unwrap();
        let genesis_hash = store.store.lock().await.head_provider().get().unwrap();

        let BlockWithSignatures { block, .. } =
            store.produce_block_with_signatures(1, 1).await.unwrap();
        assert_eq!(block.slot, 1);
        assert_eq!(block.parent_root, genesis_hash);

        let BlockWithSignatures { block, .. } =
            store.produce_block_with_signatures(2, 2).await.unwrap();

        assert_eq!(block.slot, 2);
        assert_eq!(block.parent_root, genesis_hash);
        assert!(block_provider.get(genesis_hash).unwrap().is_some());
    }

    /// Test block production with no available attestations.
    #[tokio::test]
    pub async fn test_produce_block_empty_attestations() {
        let (mut store, _) = sample_store(10).await;
        let head = store.get_proposal_head(3).await.unwrap();

        let BlockWithSignatures { block, .. } =
            store.produce_block_with_signatures(3, 3).await.unwrap();

        assert_eq!(block.body.attestations.len(), 0);
        assert_eq!(block.slot, 3);
        assert_eq!(block.parent_root, head);
        assert!(!block.state_root.is_zero());
    }

    // ATTESTATION TESTS

    /// Test basic attestation production.
    #[tokio::test]
    pub async fn test_produce_attestation_basic() {
        let slot = 1;
        let validator_id = 5;

        let (store, _) = sample_store(10).await;
        let latest_justified_checkpoint = store
            .store
            .lock()
            .await
            .latest_justified_provider()
            .get()
            .unwrap();

        let attestation = AggregatedAttestations {
            validator_id,
            data: store.produce_attestation_data(slot).await.unwrap(),
        };
        assert_eq!(attestation.validator_id, validator_id);
        assert_eq!(attestation.data.slot, slot);
        assert_eq!(attestation.data.source, latest_justified_checkpoint);
    }

    /// Test that attestation references correct head.
    #[tokio::test]
    pub async fn test_produce_attestation_head_reference() {
        let slot = 2;
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;
        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;
        let block_provider = store.store.lock().await.block_provider();
        let attestation = AggregatedAttestations {
            validator_id: 8,
            data: store.produce_attestation_data(slot).await.unwrap(),
        };
        let head = store.get_proposal_head(slot).await.unwrap();

        assert_eq!(attestation.data.head.root, head);

        let head_block = block_provider.get(head).unwrap().unwrap();
        assert_eq!(attestation.data.head.slot, head_block.message.block.slot);
    }

    /// Test that attestation calculates target correctly.
    #[tokio::test]
    pub async fn test_produce_attestation_target_calculation() {
        let (store, _) = sample_store(10).await;
        let attestation = AggregatedAttestations {
            validator_id: 9,
            data: store.produce_attestation_data(3).await.unwrap(),
        };
        let expected_target = store.get_attestation_target().await.unwrap();
        assert_eq!(attestation.data.target.root, expected_target.root);
        assert_eq!(attestation.data.target.slot, expected_target.slot);
    }

    /// Test attestation production for different validators in same slot.
    #[tokio::test]
    pub async fn test_produce_attestation_different_validators() {
        let slot = 4;
        let (store, _) = sample_store(10).await;

        let mut attestations = Vec::new();
        for validator_id in 0..5 {
            let attestation = AggregatedAttestations {
                validator_id,
                data: store.produce_attestation_data(slot).await.unwrap(),
            };

            assert_eq!(attestation.validator_id, validator_id);
            assert_eq!(attestation.data.slot, slot);

            attestations.push(attestation);
        }
        let first_attestation = &attestations[0];
        for attestation in attestations.iter().skip(1) {
            assert_eq!(attestation.data.head, first_attestation.data.head);
            assert_eq!(attestation.data.target, first_attestation.data.target);
            assert_eq!(attestation.data.source, first_attestation.data.source);
        }
    }

    /// Test attestation production across sequential slots.
    #[tokio::test]
    pub async fn test_produce_attestation_sequential_slots() {
        let (store, _) = sample_store(10).await;
        let latest_justified_provider = store.store.lock().await.latest_justified_provider();

        let mut aggregation_bits = BitList::<U4096>::with_capacity(32).unwrap();
        aggregation_bits.set(0, true).unwrap();

        let attestation_1 = AggregatedAttestation {
            aggregation_bits: aggregation_bits.clone(),
            message: store.produce_attestation_data(1).await.unwrap(),
        };

        let attestation_2 = AggregatedAttestation {
            aggregation_bits,
            message: store.produce_attestation_data(2).await.unwrap(),
        };

        assert_ne!(attestation_1.slot(), attestation_2.slot());
        assert_eq!(attestation_1.source(), attestation_2.source());
        assert_eq!(
            attestation_1.source(),
            latest_justified_provider.get().unwrap()
        );
    }

    /// Test that attestation source uses current justified checkpoint.
    #[tokio::test]
    pub async fn test_produce_attestation_justification_consistency() {
        let (store, _) = sample_store(10).await;
        let (latest_justified_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.latest_justified_provider(), db.block_provider())
        };

        let mut aggregation_bits = BitList::<U4096>::with_capacity(32).unwrap();
        aggregation_bits.set(0, true).unwrap();

        let attestation = AggregatedAttestation {
            aggregation_bits,
            message: store.produce_attestation_data(5).await.unwrap(),
        };

        assert_eq!(
            attestation.source(),
            latest_justified_provider.get().unwrap()
        );
        assert!(
            block_provider
                .get(attestation.source().root)
                .unwrap()
                .is_some()
        );
    }

    // VALIDATOR ERROR HANDLING TESTS

    /// Test error when wrong validator tries to produce block.
    #[tokio::test]
    pub async fn test_produce_block_wrong_proposer() {
        let (mut store, _) = sample_store(10).await;

        let block = store.produce_block_with_signatures(5, 3).await;
        assert!(block.is_err());
        assert_eq!(
            block.unwrap_err().to_string(),
            "Validator 3 is not the proposer for slot 5".to_string()
        );
    }

    /// Test error when parent state is missing.
    #[tokio::test]
    pub async fn test_produce_block_missing_parent_state() {
        let (mut store, _) = sample_store(10).await;
        store
            .store
            .lock()
            .await
            .head_provider()
            .insert(B256::ZERO)
            .unwrap();
        store
            .store
            .lock()
            .await
            .safe_target_provider()
            .insert(B256::ZERO)
            .unwrap();

        let block = store.produce_block_with_signatures(1, 1).await;
        assert_eq!(
            block.unwrap_err().to_string(),
            "Failed to get head state for safe target update".to_string()
        );
    }

    /// Test validator operations with invalid parameters.
    #[tokio::test]
    pub async fn test_validator_operations_invalid_parameters() {
        let (store, _) = sample_store(10).await;

        // shoudl fail
        assert!(!is_proposer(1000000, 1000000, 10));

        let attestation = AggregatedAttestations {
            validator_id: 1000000,
            data: store.produce_attestation_data(1).await.unwrap(),
        };
        assert_eq!(attestation.validator_id, 1000000);
    }

    // ON TICK TESTS

    // Test basic on_tick functionality.
    #[tokio::test]
    pub async fn test_on_tick_basic() {
        #[cfg(feature = "devnet2")]
        let (store, state) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = state.config.genesis_time + 200;

        #[cfg(feature = "devnet2")]
        store.on_tick(target_time, true).await.unwrap();

        #[cfg(feature = "devnet3")]
        store.on_tick(target_time, true, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time > initial_time);
    }

    // Test on_tick without proposal.
    #[tokio::test]
    pub async fn test_on_tick_no_proposal() {
        #[cfg(feature = "devnet2")]
        let (store, state) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = state.config.genesis_time + 100;

        #[cfg(feature = "devnet2")]
        store.on_tick(target_time, true).await.unwrap();

        #[cfg(feature = "devnet3")]
        store.on_tick(target_time, true, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    // Test on_tick when already at target time.
    #[tokio::test]
    pub async fn test_on_tick_already_current() {
        #[cfg(feature = "devnet2")]
        let (store, state) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let current_target = state.config.genesis_time + initial_time;

        #[cfg(feature = "devnet2")]
        store.on_tick(current_target, false).await.unwrap();

        #[cfg(feature = "devnet3")]
        store.on_tick(current_target, false, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time - initial_time <= 10);
    }

    // Test on_tick with small time increment.
    #[tokio::test]
    pub async fn test_on_tick_small_increment() {
        #[cfg(feature = "devnet2")]
        let (store, state) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = state.config.genesis_time + initial_time + 1;

        #[cfg(feature = "devnet2")]
        store.on_tick(target_time, false).await.unwrap();

        #[cfg(feature = "devnet3")]
        store.on_tick(target_time, false, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    // TEST INTERVAL TICKING

    // Test basic interval ticking.
    #[tokio::test]
    pub async fn test_tick_interval_basic() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        #[cfg(feature = "devnet2")]
        store.tick_interval(false).await.unwrap();

        #[cfg(feature = "devnet3")]
        store.tick_interval(false, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 1)
    }

    // Test interval ticking with proposal.
    #[tokio::test]
    pub async fn test_tick_interval_with_proposal() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        #[cfg(feature = "devnet2")]
        store.tick_interval(true).await.unwrap();

        #[cfg(feature = "devnet3")]
        store.tick_interval(true, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 1)
    }

    // Test sequence of interval ticks.
    #[tokio::test]
    pub async fn test_tick_interval_sequence() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        #[cfg(feature = "devnet2")]
        for i in 0..5 {
            store.tick_interval((i % 2) == 0).await.unwrap();
        }

        #[cfg(feature = "devnet3")]
        for i in 0..5 {
            store.tick_interval((i % 2) == 0, false).await.unwrap();
        }

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 5)
    }

    // Test different actions performed based on interval phase.
    #[tokio::test]
    pub async fn test_tick_interval_actions_by_phase() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;

        let mut root = [0u8; 32];
        root[..4].copy_from_slice(b"test");
        let test_checkpoint = Checkpoint {
            slot: 1,
            root: FixedBytes::new(root),
        };

        {
            let db = store.store.lock().await;
            let justified_provider = db.latest_justified_provider();
            let justified_checkpoint = justified_provider.get().unwrap();
            let signed_attestation = SignedAttestation {
                message: AttestationData {
                    slot: 1,
                    head: justified_checkpoint,
                    target: test_checkpoint,
                    source: justified_checkpoint,
                },
                validator_id: 5,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();
            db_table
                .insert(signed_attestation.validator_id, signed_attestation)
                .unwrap();
        };

        for interval in 0..INTERVALS_PER_SLOT {
            let has_proposal = interval == 0;
            #[cfg(feature = "devnet2")]
            store.tick_interval(has_proposal).await.unwrap();
            #[cfg(feature = "devnet3")]
            store.tick_interval(has_proposal, false).await.unwrap();

            let new_time = {
                let time_provider = store.store.lock().await.time_provider();
                time_provider.get().unwrap()
            };
            let current_interval = new_time % INTERVALS_PER_SLOT;
            let expected_interval = (interval + 1) % INTERVALS_PER_SLOT;

            assert!(current_interval == expected_interval);
        }
    }

    // TEST SLOT TIME CALCULATIONS

    // Test conversion from slot to time.
    #[tokio::test]
    pub async fn test_slot_to_time_conversion() {
        let (_, state) = sample_store(10).await;

        let genesis_time = state.config.genesis_time;

        let slot_0_time = genesis_time;
        assert!(slot_0_time == genesis_time);

        let slot_1_time = genesis_time + lean_network_spec().seconds_per_slot;
        assert!(slot_1_time == genesis_time + lean_network_spec().seconds_per_slot);

        let slot_10_time = genesis_time + 10 * lean_network_spec().seconds_per_slot;
        assert!(slot_10_time == genesis_time + 10 * lean_network_spec().seconds_per_slot);
    }

    // Test conversion from time to slot.
    #[tokio::test]
    pub async fn test_time_to_slot_conversion() {
        let (_, state) = sample_store(10).await;

        let genesis_time = state.config.genesis_time;

        let time_at_genesis = genesis_time;
        let slot_0 = (time_at_genesis - genesis_time) / lean_network_spec().seconds_per_slot;
        assert!(slot_0 == 0);

        let time_after_one_slot = genesis_time + lean_network_spec().seconds_per_slot;
        let slot_1 = (time_after_one_slot - genesis_time) / lean_network_spec().seconds_per_slot;
        assert!(slot_1 == 1);

        let time_after_five_slots = genesis_time + 5 * lean_network_spec().seconds_per_slot;
        let slot_5 = (time_after_five_slots - genesis_time) / lean_network_spec().seconds_per_slot;
        assert!(slot_5 == 5);
    }

    // Test interval calculations within slots.
    #[tokio::test]
    pub async fn test_interval_calculations() {
        let total_intervals = 10;
        let slot_number = total_intervals / INTERVALS_PER_SLOT;
        let interval_in_slot = total_intervals % INTERVALS_PER_SLOT;

        assert!(slot_number == 2);
        assert!(interval_in_slot == 2);

        let boundary_intervals = INTERVALS_PER_SLOT;
        let boundary_slot = boundary_intervals / INTERVALS_PER_SLOT;
        let boundary_interval = boundary_intervals % INTERVALS_PER_SLOT;

        assert!(boundary_slot == 1);
        assert!(boundary_interval == 0);
    }

    // TEST ATTESTATION PROCESSING TIMING

    // Test basic new attestation processing.
    #[tokio::test]
    pub async fn test_accept_new_attestations_basic() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;

        let mut root = [0u8; 32];
        root[..4].copy_from_slice(b"test");
        let checkpoint = Checkpoint {
            slot: 1,
            root: FixedBytes::new(root),
        };
        {
            let db = store.store.lock().await;
            let justified_provider = db.latest_justified_provider();
            let justified_checkpoint = justified_provider.get().unwrap();
            let signed_attestation = SignedAttestation {
                message: AttestationData {
                    slot: 1,
                    head: justified_checkpoint,
                    target: checkpoint,
                    source: justified_checkpoint,
                },
                validator_id: 5,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();
            db_table
                .insert(signed_attestation.validator_id, signed_attestation)
                .unwrap();
        };
        let latest_new_attestations_provider =
            { store.store.lock().await.latest_new_attestations_provider() };
        let latest_known_attestations_provider = {
            store
                .store
                .lock()
                .await
                .latest_known_attestations_provider()
        };

        let inititial_new_attestations_length = latest_new_attestations_provider
            .iter_values()
            .unwrap()
            .count();
        let initial_known_attestations_length = latest_known_attestations_provider
            .get_all_attestations()
            .unwrap()
            .keys()
            .len();

        store.accept_new_attestations().await.unwrap();

        let final_new_attestations_length = latest_new_attestations_provider
            .iter_values()
            .unwrap()
            .count();
        let final_latest_known_attestations_length = latest_known_attestations_provider
            .get_all_attestations()
            .unwrap()
            .keys()
            .len();

        assert!(final_new_attestations_length == 0);
        assert!(
            final_latest_known_attestations_length
                == initial_known_attestations_length + inititial_new_attestations_length
        );
    }

    // Test accepting multiple new attestations.
    #[tokio::test]
    pub async fn test_accept_new_attestations_multiple() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;

        let mut checkpoints: Vec<Checkpoint> = Vec::new();
        for i in 0..5 {
            let root = {
                let mut root_vec = [0u8; 32];
                root_vec[..4].copy_from_slice(b"test");
                root_vec[5] = i;
                FixedBytes::new(root_vec)
            };

            checkpoints.push(Checkpoint {
                root,
                slot: i.into(),
            });
        }

        for (i, checkpoint) in checkpoints.iter().enumerate().map(|(i, c)| (i as u64, c)) {
            let db = store.store.lock().await;
            let justified_provider = db.latest_justified_provider();
            let justified_checkpoint = justified_provider.get().unwrap();
            let signed_attestation = SignedAttestation {
                message: AttestationData {
                    slot: i,
                    head: justified_checkpoint,
                    target: *checkpoint,
                    source: justified_checkpoint,
                },
                validator_id: i,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();

            db_table
                .insert(signed_attestation.validator_id, signed_attestation)
                .unwrap();
        }

        let latest_known_attestations_provider = {
            store
                .store
                .lock()
                .await
                .latest_known_attestations_provider()
        };

        store.accept_new_attestations().await.unwrap();

        let new_attestations_length = {
            store
                .store
                .lock()
                .await
                .latest_new_attestations_provider()
                .iter_values()
                .unwrap()
                .count()
        };
        let latest_known_attestations_length = latest_known_attestations_provider
            .get_all_attestations()
            .unwrap()
            .keys()
            .len();

        assert!(new_attestations_length == 0);
        assert!(latest_known_attestations_length == 5);

        for (i, checkpoint) in checkpoints.iter().enumerate().map(|(i, c)| (i as u64, c)) {
            let stored_checkpoint = latest_known_attestations_provider
                .get(i)
                .unwrap()
                .unwrap()
                .message
                .target;
            assert!(stored_checkpoint == *checkpoint);
        }
    }

    // Test accepting new attestations when there are none.
    #[tokio::test]
    pub async fn test_accept_new_attestations_empty() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;

        let latest_known_attestations_provider = {
            store
                .store
                .lock()
                .await
                .latest_known_attestations_provider()
        };

        let initial_known_attestations_length = latest_known_attestations_provider
            .get_all_attestations()
            .unwrap()
            .keys()
            .len();

        store.accept_new_attestations().await.unwrap();

        let final_new_attestations_length = {
            store
                .store
                .lock()
                .await
                .latest_new_attestations_provider()
                .iter_values()
                .unwrap()
                .count()
        };
        let latest_known_attestations_length = latest_known_attestations_provider
            .get_all_attestations()
            .unwrap()
            .keys()
            .len();

        assert!(final_new_attestations_length == 0);
        assert!(latest_known_attestations_length == initial_known_attestations_length);
    }

    // TEST PROPOSAL HEAD TIMING

    // Test getting proposal head for a slot.
    #[tokio::test]
    pub async fn test_get_proposal_head_basic() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;

        let head = store.get_proposal_head(0).await.unwrap();

        let stored_head = { store.store.lock().await.head_provider().get().unwrap() };

        assert!(head == stored_head);
    }

    // Test that get_proposal_head advances store time appropriately.
    #[tokio::test]
    pub async fn test_get_proposal_head_advances_time() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        store.get_proposal_head(5).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    // Test that get_proposal_head processes pending attestations.
    #[tokio::test]
    pub async fn test_get_proposal_head_processes_attestations() {
        #[cfg(feature = "devnet2")]
        let (store, _) = sample_store(10).await;

        #[cfg(feature = "devnet3")]
        let (mut store, _) = sample_store(10).await;

        let root = {
            let mut root_vec = [0u8; 32];
            root_vec[..11].copy_from_slice(b"attestation");
            FixedBytes::new(root_vec)
        };
        let checkpoint = Checkpoint { slot: 10, root };

        {
            let db = store.store.lock().await;
            let justified_provider = db.latest_justified_provider();
            let justified_checkpoint = justified_provider.get().unwrap();
            let signed_attestation = SignedAttestation {
                message: AttestationData {
                    slot: 10,
                    head: justified_checkpoint,
                    target: checkpoint,
                    source: justified_checkpoint,
                },
                validator_id: 10,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();
            db_table
                .insert(signed_attestation.validator_id, signed_attestation)
                .unwrap();
        };

        store.get_proposal_head(1).await.unwrap();

        let new_attestations_length = {
            store
                .store
                .lock()
                .await
                .latest_new_attestations_provider()
                .iter_values()
                .unwrap()
                .count()
        };

        let known_attestations_correct_checkpoint = {
            store
                .store
                .lock()
                .await
                .latest_known_attestations_provider()
                .get_all_attestations()
                .unwrap()
                .get(&10)
                .unwrap()
                .message
                .target
        };

        assert!(new_attestations_length == 0);
        assert!(known_attestations_correct_checkpoint.slot == 10);
        assert!(known_attestations_correct_checkpoint == checkpoint);
    }

    // TEST TIME CONSTANTS

    // Test that time constants are consistent with each other.
    #[allow(clippy::assertions_on_constants)]
    #[tokio::test]
    pub async fn test_time_constants_consistency() {
        set_lean_network_spec(LeanNetworkSpec::ephemery().into());
        let seconds_per_interval = lean_network_spec().seconds_per_slot / INTERVALS_PER_SLOT;

        assert!(INTERVALS_PER_SLOT > 0);
        assert!(seconds_per_interval > 0);
        assert!(lean_network_spec().seconds_per_slot > 0);
    }

    // Test the relationship between intervals and slots.
    #[allow(clippy::assertions_on_constants)]
    #[tokio::test]
    pub async fn test_interval_slot_relationship() {
        assert!(INTERVALS_PER_SLOT >= 2);

        let total_intervals = 100;
        let complete_slots = total_intervals / INTERVALS_PER_SLOT;
        let remaining_intervals = total_intervals % INTERVALS_PER_SLOT;

        let reconstructed = complete_slots * INTERVALS_PER_SLOT + remaining_intervals;
        assert!(reconstructed == total_intervals);
    }
}
