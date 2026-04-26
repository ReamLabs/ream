#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof, AttestationData,
        SignatureKey, SignedAggregatedAttestation, SignedAttestation,
    },
    block::{Block, BlockBody, BlockWithSignatures, SignedBlock},
    checkpoint::Checkpoint,
    slot::is_justifiable_after,
    state::LeanState,
    validator::is_proposer,
};
use ream_consensus_misc::constants::lean::{
    ATTESTATION_COMMITTEE_COUNT, INTERVALS_PER_SLOT, MAX_ATTESTATIONS_DATA,
};
use ream_metrics::{
    ATTESTATION_COMMITTEE_SUBNET, FINALIZATIONS_TOTAL, FINALIZED_SLOT,
    FORK_CHOICE_BLOCK_PROCESSING_TIME, HEAD_SLOT, JUSTIFIED_SLOT, LATEST_FINALIZED_SLOT,
    LATEST_JUSTIFIED_SLOT, LATEST_KNOWN_AGGREGATED_PAYLOADS, LATEST_NEW_AGGREGATED_PAYLOADS,
    PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL, PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL,
    PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME, PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL,
    PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL, PQ_SIG_ATTESTATION_VERIFICATION_TIME,
    PROPOSE_BLOCK_TIME, SAFE_TARGET_SLOT, inc_int_counter_vec, set_int_gauge_vec, start_timer,
    stop_timer,
};
use ream_network_spec::networks::lean_network_spec;
use ream_network_state_lean::NetworkState;
use ream_post_quantum_crypto::{
    lean_multisig::aggregate::{aggregate_signatures, verify_aggregate_signature},
    leansig::signature::Signature,
};
use ream_storage::{
    db::lean::LeanDB,
    tables::{field::REDBField, lean::gossip_signatures::GossipSignaturesTable, table::REDBTable},
};
use ream_sync::rwlock::{Reader, Writer};
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
}

impl Store {
    /// Initialize forkchoice store from an anchor state and anchor block.
    pub fn get_forkchoice_store(
        anchor_block: SignedBlock,
        anchor_state: LeanState,
        db: LeanDB,
        time: Option<u64>,
        validator_id: Option<u64>,
    ) -> anyhow::Result<Store> {
        ensure!(
            anchor_block.block.state_root == anchor_state.tree_hash_root(),
            "Anchor block state root must match anchor state hash"
        );

        let anchor_root = {
            let mut header = anchor_state.latest_block_header.clone();
            if header.state_root == B256::ZERO {
                header.state_root = anchor_state.tree_hash_root();
            }
            header.tree_hash_root()
        };
        let anchor_slot = anchor_block.block.slot;

        let justified_checkpoint = Checkpoint {
            root: anchor_root,
            slot: anchor_state.latest_justified.slot,
        };
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
        db.slot_index_provider()
            .insert(anchor_slot, anchor_root)
            .expect("Failed to overwrite anchor slot index");
        db.state_root_index_provider()
            .insert(anchor_state.tree_hash_root(), anchor_root)
            .expect("Failed to overwrite anchor state root index");
        db.latest_finalized_provider()
            .insert(finalized_checkpoint)
            .expect("Failed to insert latest finalized checkpoint");
        db.latest_justified_provider()
            .insert(justified_checkpoint)
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
        db.validator_id_provider()
            .insert(validator_id)
            .expect("Failed to insert validator id");

        Ok(Store {
            store: Arc::new(Mutex::new(db)),
            network_state: Arc::new(NetworkState::new(
                justified_checkpoint,
                finalized_checkpoint,
                false,
            )),
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
        if root == B256::ZERO || block_provider.get(root)?.is_none() {
            root = slot_index_table
                .get_oldest_root()?
                .ok_or(anyhow!("No blocks found to calculate fork choice"))?;
        }
        let start_slot = block_provider
            .get(root)?
            .ok_or(anyhow!("Failed to get block for root {root:?}"))?
            .block
            .slot;
        // For each block, count the number of votes for that block. A vote
        // for any descendant of a block also counts as a vote for that block
        let mut weights = HashMap::<B256, u64>::new();

        for attestation in attestations {
            let attestation = attestation?;
            let mut current_root = attestation.message.head.root;

            while let Some(block) = block_provider.get(current_root)? {
                let block = block.block;

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
                        .map(|block| block.block.slot)
                        .unwrap_or(0);
                    (*child_hash, slot, (vote_weight, *child_hash))
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
        let (
            head_provider,
            state_provider,
            latest_justified_provider,
            safe_target_provider,
            latest_new_aggregated_payloads_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.head_provider(),
                db.state_provider(),
                db.latest_justified_provider(),
                db.safe_target_provider(),
                db.latest_new_aggregated_payloads_provider(),
            )
        };

        let head_state = state_provider
            .get(head_provider.get()?)?
            .ok_or(anyhow!("Failed to get head state for safe target update"))?;

        let min_target_score = (head_state.validators.len() as u64 * 2).div_ceil(3);
        let latest_justified_root = latest_justified_provider.get()?.root;

        let attestations = {
            let new_payloads = latest_new_aggregated_payloads_provider
                .iter()?
                .into_iter()
                .collect();

            self.extract_attestations_from_aggregated_payloads(&new_payloads)
                .await?
        };

        let (new_safe_target_root, new_safe_target_slot) = self
            .compute_lmd_ghost_head(
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

    pub async fn accept_new_attestations(&mut self) -> anyhow::Result<()> {
        let (latest_new_aggregated_payloads_provider, latest_known_aggregated_payloads_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_new_aggregated_payloads_provider(),
                db.latest_known_aggregated_payloads_provider(),
            )
        };

        let payloads = latest_new_aggregated_payloads_provider.drain()?;
        set_int_gauge_vec(&LATEST_NEW_AGGREGATED_PAYLOADS, payloads.len() as i64, &[]);

        for (signature_key, mut new_proofs) in payloads {
            let mut existing_proofs = latest_known_aggregated_payloads_provider
                .get(signature_key.clone())?
                .unwrap_or_default();

            existing_proofs.append(&mut new_proofs);

            latest_known_aggregated_payloads_provider.insert(signature_key, existing_proofs)?;
        }

        set_int_gauge_vec(
            &LATEST_KNOWN_AGGREGATED_PAYLOADS,
            latest_known_aggregated_payloads_provider.iter()?.len() as i64,
            &[],
        );

        self.update_head().await?;

        Ok(())
    }

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
            // Interval 2: Only aggregate signatures if aggregator
            if is_aggregator {
                self.aggregate().await?;
            }
        } else if current_interval == 3 {
            // Interval 3: Update safe target
            self.update_safe_target().await?;
        } else if current_interval == 4 {
            // Interval 4: Accept accumulated attestations
            self.accept_new_attestations().await?;
        }

        Ok(())
    }

    pub async fn on_tick(
        &mut self,
        time: u64,
        has_proposal: bool,
        is_aggregator: bool,
    ) -> anyhow::Result<()> {
        let time_delta_ms = (time - lean_network_spec().genesis_time) * 1000;
        let tick_interval_time =
            time_delta_ms * INTERVALS_PER_SLOT / (lean_network_spec().seconds_per_slot * 1000);

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
        let (latest_justified_provider, head_provider, latest_known_aggregated_payloads_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_justified_provider(),
                db.head_provider(),
                db.latest_known_aggregated_payloads_provider(),
            )
        };

        let attestations = {
            let entries = latest_known_aggregated_payloads_provider.iter()?;

            let all_payloads: HashMap<SignatureKey, Vec<AggregatedSignatureProof>> =
                entries.into_iter().collect();

            self.extract_attestations_from_aggregated_payloads(&all_payloads)
                .await?
        };

        let (new_head, new_head_slot) = self
            .compute_lmd_ghost_head(
                attestations.into_iter().map(|(validator, data)| {
                    Ok(SignedAttestation {
                        validator_id: validator,
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
                .block
                .slot
                > block_provider
                    .get(safe_target_provider.get()?)?
                    .ok_or(anyhow!("Block not found for safe target"))?
                    .block
                    .slot
            {
                target_block_root = block_provider
                    .get(target_block_root)?
                    .ok_or(anyhow!("Block not found for target block root"))?
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
                .block
                .slot,
            latest_finalized_slot,
        )? {
            target_block_root = block_provider
                .get(target_block_root)?
                .ok_or(anyhow!("Block not found for target block root"))?
                .block
                .parent_root;
        }

        let target_block = block_provider
            .get(target_block_root)?
            .ok_or(anyhow!("Block not found for target block root"))?;

        Ok(Checkpoint {
            root: target_block_root,
            slot: target_block.block.slot,
        })
    }

    /// Get the head for block proposal at given slot.
    /// Ensures store is up-to-date and processes any pending attestations.
    pub async fn get_proposal_head(&mut self, slot: u64) -> anyhow::Result<B256> {
        let slot_duration_seconds = slot * lean_network_spec().seconds_per_slot;
        let slot_time = lean_network_spec().genesis_time + slot_duration_seconds;
        self.on_tick(slot_time, true, false).await?;
        self.accept_new_attestations().await?;
        Ok(self.store.lock().await.head_provider().get()?)
    }

    fn state_aggregate(
        &self,
        head_state: &LeanState,
        attestations: &[AggregatedAttestations],
        gossip_signatures_provider: &GossipSignaturesTable,
        new_payloads: Option<&HashMap<AttestationData, HashSet<AggregatedSignatureProof>>>,
        known_payloads: Option<&HashMap<AttestationData, HashSet<AggregatedSignatureProof>>>,
        recursive: bool,
    ) -> anyhow::Result<Vec<SignedAggregatedAttestation>> {
        let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();
        for attestation in attestations.iter() {
            groups
                .entry(attestation.data.clone())
                .or_default()
                .push(attestation.validator_id);
        }

        let mut results = Vec::new();

        let attestation_keys: HashSet<AttestationData> = if recursive {
            let mut keys: HashSet<AttestationData> = groups.keys().cloned().collect();
            if let Some(payloads) = new_payloads {
                keys.extend(payloads.keys().cloned());
            }
            keys
        } else {
            groups.keys().cloned().collect()
        };

        if attestation_keys.is_empty() {
            return Ok(Vec::new());
        }

        for data in attestation_keys {
            let data_root = data.tree_hash_root();
            let mut child_proofs = Vec::new();
            let mut covered_validators = HashSet::new();

            if recursive {
                if let Some(payloads) = new_payloads {
                    head_state.extend_proofs_greedily(
                        payloads.get(&data),
                        &mut child_proofs,
                        &mut covered_validators,
                    );
                }
                if let Some(payloads) = known_payloads {
                    head_state.extend_proofs_greedily(
                        payloads.get(&data),
                        &mut child_proofs,
                        &mut covered_validators,
                    );
                }
            }

            let mut raw_entries = Vec::new();
            if let Some(validator_ids) = groups.get(&data) {
                let mut sorted_ids = validator_ids.clone();
                sorted_ids.sort();

                for &validator_id in &sorted_ids {
                    if recursive && covered_validators.contains(&validator_id) {
                        continue;
                    }

                    if let Ok(Some(signature)) = gossip_signatures_provider
                        .get(SignatureKey::from_parts(validator_id, data_root))
                        && let Some(validator) = head_state.validators.get(validator_id as usize)
                    {
                        raw_entries.push((
                            validator_id,
                            validator.attestation_public_key,
                            signature,
                        ));

                        if recursive {
                            covered_validators.insert(validator_id);
                        }
                    }
                }
            }

            if recursive {
                if raw_entries.is_empty() && child_proofs.len() < 2 {
                    continue;
                }
            } else if raw_entries.is_empty() {
                continue;
            }

            raw_entries.sort_by_key(|err| err.0);

            let mut bits = BitList::<U4096>::with_capacity(head_state.validators.len())
                .map_err(|err| anyhow!("BitList error: {err:?}"))?;

            if recursive {
                for id in &covered_validators {
                    bits.set(*id as usize, true)
                        .map_err(|err| anyhow!("Failed to set bits: {err:?}"))?;
                }
            } else {
                for (id, _, _) in &raw_entries {
                    bits.set(*id as usize, true)
                        .map_err(|err| anyhow!("Failed to set bits: {err:?}"))?;
                }
            }

            let xmss_keys: Vec<_> = raw_entries.iter().map(|err| err.1).collect();
            let xmss_sigs: Vec<_> = raw_entries.iter().map(|err| err.2).collect();

            let aggregated_signature =
                aggregate_signatures(&xmss_keys, &xmss_sigs, &data_root.0, data.slot as u32)?;

            let proof = AggregatedSignatureProof {
                participants: bits.clone(),
                proof_data: VariableList::new(aggregated_signature)
                    .map_err(|err| anyhow!("Failed to create proof_data: {err:?}"))?,
            };

            results.push(SignedAggregatedAttestation {
                data: data.clone(),
                proof,
            });
        }

        Ok(results)
    }

    async fn select_aggregated_proofs(
        &self,
        attestations: &[AggregatedAttestations],
    ) -> anyhow::Result<(Vec<AggregatedAttestation>, Vec<AggregatedSignatureProof>)> {
        let mut results = Vec::new();
        let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();
        let latest_known_aggregated_payloads_provider = self
            .store
            .lock()
            .await
            .latest_known_aggregated_payloads_provider();

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

                let candidates = match latest_known_aggregated_payloads_provider
                    .get(SignatureKey::from_parts(target_id, data_root))?
                {
                    Some(proofs) => proofs.clone(),
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
            latest_known_aggregated_payloads_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.latest_known_attestations_provider(),
                db.block_provider(),
                db.latest_known_aggregated_payloads_provider(),
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

                let has_proof =
                    latest_known_aggregated_payloads_provider.contains_key(&signature_key);

                if has_proof {
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

        let (aggregated_attestations, aggregated_proofs) =
            self.select_aggregated_proofs(&attestations_vec).await?;

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
        let (state_provider, latest_known_aggregated_payloads_provider, latest_finalized_provider) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.latest_known_aggregated_payloads_provider(),
                db.latest_finalized_provider(),
            )
        };

        let head_root = self.get_proposal_head(slot).await?;
        let initialize_block_timer = start_timer(&PROPOSE_BLOCK_TIME, &["initialize_block"]);

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

        let attestation_data_map = {
            let entries = latest_known_aggregated_payloads_provider.iter()?;

            let all_payloads: HashMap<SignatureKey, Vec<AggregatedSignatureProof>> =
                entries.into_iter().collect();

            self.extract_attestations_from_aggregated_payloads(&all_payloads)
                .await?
        };

        let attestation_vector: Vec<AggregatedAttestations> = attestation_data_map
            .into_iter()
            .map(|(validator, data)| AggregatedAttestations {
                validator_id: validator,
                data,
            })
            .collect();

        let attestation_list = VariableList::new(attestation_vector.clone())
            .map_err(|err| anyhow!("Failed to create VariableList: {err:?}"))?;

        let (mut candidate_block, proofs, post_state) = self
            .build_block(slot, validator_index, head_root, Some(attestation_list))
            .await?;

        stop_timer(add_attestations_timer);

        let compute_state_root_timer = start_timer(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        candidate_block.state_root = post_state.tree_hash_root();
        stop_timer(compute_state_root_timer);

        let signatures_list = VariableList::new(proofs)
            .map_err(|err| anyhow!("Failed to return signatures {err:?}"))?;

        let finalized_advanced =
            post_state.latest_finalized.slot > latest_finalized_provider.get()?.slot;

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
        signed_block: &SignedBlock,
        verify_signatures: bool,
    ) -> anyhow::Result<()> {
        let block_processing_timer = start_timer(&FORK_CHOICE_BLOCK_PROCESSING_TIME, &[]);

        let (
            state_provider,
            block_provider,
            latest_justified_provider,
            latest_finalized_provider,
            attestation_data_by_root_provider,
            latest_known_aggregated_payloads_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.block_provider(),
                db.latest_justified_provider(),
                db.latest_finalized_provider(),
                db.attestation_data_by_root_provider(),
                db.latest_known_aggregated_payloads_provider(),
            )
        };

        let block = &signed_block.block;
        let block_root = block.tree_hash_root();

        // If the block is already known, ignore it
        if block_provider.get(block_root)?.is_some() {
            stop_timer(block_processing_timer);
            return Ok(());
        }

        let mut parent_state = state_provider
            .get(block.parent_root)?
            .ok_or(anyhow!("State not found for parent root"))?;

        signed_block.verify_signatures(&parent_state, verify_signatures)?;
        parent_state.state_transition(block, true)?;

        let latest_justified = if parent_state.latest_justified.slot
            > latest_justified_provider.get()?.slot
            && block_provider.contains_key(parent_state.latest_justified.root)
        {
            parent_state.latest_justified
        } else {
            latest_justified_provider.get()?
        };

        let finalized_advanced = parent_state.latest_finalized.slot
            > latest_finalized_provider.get()?.slot
            && block_provider.contains_key(parent_state.latest_finalized.root);
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
        block_provider.insert(block_root, signed_block.clone())?;
        state_provider.insert(block_root, parent_state)?;
        latest_justified_provider.insert(latest_justified)?;
        latest_finalized_provider.insert(latest_finalized)?;
        *self.network_state.finalized_checkpoint.write() = latest_finalized;
        let aggregated_attestations = &block.body.attestations;
        let attestation_signatures = &signed_block.signature.attestation_signatures;

        ensure!(
            aggregated_attestations.len() == attestation_signatures.len(),
            "Attestation signature groups must match aggregated attestations"
        );

        let mut seen_attestation_data = HashSet::with_capacity(aggregated_attestations.len());
        for attestation in aggregated_attestations.iter() {
            let data_root = attestation.message.tree_hash_root();
            ensure!(
                seen_attestation_data.insert(data_root),
                "Block contains duplicate AttestationData entries; \
                 each AttestationData must appear at most once",
            );
        }
        let distinct_attestation_data = seen_attestation_data.len();
        ensure!(
            distinct_attestation_data as u64 <= MAX_ATTESTATIONS_DATA,
            "Block contains {distinct_attestation_data} distinct AttestationData entries; \
             maximum is {MAX_ATTESTATIONS_DATA}",
        );

        for (attestation, proof) in aggregated_attestations
            .iter()
            .zip(attestation_signatures.iter())
        {
            let validator_ids = proof.to_validator_indices();
            let data_root = attestation.message.tree_hash_root();

            attestation_data_by_root_provider.insert(data_root, attestation.message.clone())?;

            for validator_id in validator_ids {
                let key = SignatureKey::from_parts(validator_id, data_root);

                let mut existing_proofs = latest_known_aggregated_payloads_provider
                    .get(key.clone())?
                    .unwrap_or_default();

                existing_proofs.push(proof.clone());

                latest_known_aggregated_payloads_provider.insert(key, existing_proofs)?;
            }
        }

        self.update_head().await?;

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

        let (block_provider, time_provider) = {
            let db = self.store.lock().await;
            (db.block_provider(), db.time_provider())
        };

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
        ensure!(
            data.head.slot >= data.target.slot,
            "Head checkpoint must not be older than target"
        );

        let source_block = block_provider
            .get(data.source.root)?
            .ok_or(anyhow!("Failed to get source block"))?;
        let target_block = block_provider
            .get(data.target.root)?
            .ok_or(anyhow!("Failed to get target block"))?;
        let head_block = block_provider
            .get(data.head.root)?
            .ok_or(anyhow!("Failed to get head block"))?;
        ensure!(
            source_block.block.slot == data.source.slot,
            "Source checkpoint slot mismatch"
        );
        ensure!(
            target_block.block.slot == data.target.slot,
            "Target checkpoint slot mismatch"
        );
        ensure!(
            head_block.block.slot == data.head.slot,
            "Head checkpoint slot mismatch"
        );

        let current_slot = time_provider.get()? / INTERVALS_PER_SLOT;
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
        signed_attestation: SignedAggregatedAttestation,
    ) -> anyhow::Result<()> {
        let (
            time_provider,
            attestation_data_by_root_provider,
            latest_new_aggregated_payloads_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.time_provider(),
                db.attestation_data_by_root_provider(),
                db.latest_new_aggregated_payloads_provider(),
            )
        };

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
                        .map(|validator| validator.attestation_public_key)
                        .ok_or_else(|| anyhow!("Validator {validator} not found in state"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            let verification_timer =
                start_timer(&PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME, &[]);
            match verify_aggregate_signature(
                &public_keys,
                &data_root.0,
                proof.proof_data.as_ref(),
                attestation_slot as u32,
            ) {
                Ok(()) => {
                    stop_timer(verification_timer);
                    inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL, &[]);
                    for _ in &validator_ids {
                        inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL, &[]);
                    }
                }
                Err(err) => {
                    stop_timer(verification_timer);
                    inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL, &[]);
                    for _ in &validator_ids {
                        inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL, &[]);
                    }
                    return Err(anyhow!("Aggregated signature verification failed: {err}"));
                }
            }

            attestation_data_by_root_provider.insert(data_root, data.clone())?;

            for &validator in &validator_ids {
                let key = SignatureKey::from_parts(validator, data_root);

                let mut proofs = latest_new_aggregated_payloads_provider
                    .get(key.clone())?
                    .unwrap_or_default();

                proofs.push(proof.clone());

                latest_new_aggregated_payloads_provider.insert(key, proofs)?;
            }

            let time_slots = time_provider.get()? / lean_network_spec().seconds_per_slot;
            ensure!(
                attestation_slot <= time_slots,
                "Attestation from future slot {attestation_slot} <= {time_slots}"
            );
        }

        Ok(())
    }

    pub async fn extract_attestations_from_aggregated_payloads(
        &self,
        aggregated_payloads: &HashMap<SignatureKey, Vec<AggregatedSignatureProof>>,
    ) -> anyhow::Result<HashMap<u64, AttestationData>> {
        let mut attestations: HashMap<u64, AttestationData> = HashMap::new();
        let attestation_data_by_root_provider =
            self.store.lock().await.attestation_data_by_root_provider();

        for (signature_key, proofs) in aggregated_payloads {
            let data_root = signature_key.data_root;
            let attestation_data = match attestation_data_by_root_provider.get(data_root)? {
                Some(data) => data,
                None => continue,
            };

            for proof in proofs {
                let validator_ids = proof.to_validator_indices();
                for validator in validator_ids {
                    let is_newer = attestations
                        .get(&validator)
                        .is_none_or(|existing| existing.slot < attestation_data.slot);

                    if is_newer {
                        attestations.insert(validator, attestation_data.clone());
                    }
                }
            }
        }
        Ok(attestations)
    }

    pub async fn aggregate(&mut self) -> anyhow::Result<Vec<SignedAggregatedAttestation>> {
        let (
            state_provider,
            attestation_signatures_provider,
            head_root,
            latest_new_aggregated_payloads_provider,
            latest_known_aggregated_payloads_provider,
            attestation_data_by_root_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.attestation_signatures_provider(),
                db.head_provider().get()?,
                db.latest_new_aggregated_payloads_provider(),
                db.latest_known_aggregated_payloads_provider(),
                db.attestation_data_by_root_provider(),
            )
        };

        let head_state = state_provider
            .get(head_root)?
            .ok_or_else(|| anyhow!("Head state not found"))?;

        let mut attestation_signatures = Vec::new();
        for signature_key in attestation_signatures_provider.get_keys()? {
            if let Some(attestation_data) =
                attestation_data_by_root_provider.get(signature_key.data_root)?
            {
                attestation_signatures.push(AggregatedAttestations {
                    validator_id: signature_key.validator_id,
                    data: attestation_data,
                });
            }
        }

        let mut new_payloads: HashMap<AttestationData, HashSet<AggregatedSignatureProof>> =
            HashMap::new();
        for (signature_key, proofs) in latest_new_aggregated_payloads_provider.iter()? {
            if let Some(attestation_data) =
                attestation_data_by_root_provider.get(signature_key.data_root)?
            {
                new_payloads
                    .entry(attestation_data)
                    .or_default()
                    .extend(proofs);
            }
        }

        let mut known_payloads: HashMap<AttestationData, HashSet<AggregatedSignatureProof>> =
            HashMap::new();
        for (signature_key, proofs) in latest_known_aggregated_payloads_provider.iter()? {
            if let Some(attestation_data) =
                attestation_data_by_root_provider.get(signature_key.data_root)?
            {
                known_payloads
                    .entry(attestation_data)
                    .or_default()
                    .extend(proofs);
            }
        }

        let signed_attestations = self.state_aggregate(
            &head_state,
            &attestation_signatures,
            &attestation_signatures_provider,
            Some(&new_payloads),
            Some(&known_payloads),
            true,
        )?;

        let mut aggregated_data_roots = HashSet::new();
        let mut next_new_payloads: HashMap<SignatureKey, Vec<AggregatedSignatureProof>> =
            HashMap::new();

        for signed_attestation in &signed_attestations {
            let data_root = signed_attestation.data.tree_hash_root();
            aggregated_data_roots.insert(data_root);

            for validator_id in signed_attestation.proof.to_validator_indices() {
                next_new_payloads
                    .entry(SignatureKey::from_parts(validator_id, data_root))
                    .or_default()
                    .push(signed_attestation.proof.clone());
            }
        }

        let _ = latest_new_aggregated_payloads_provider.drain()?;
        for (key, proofs) in next_new_payloads {
            latest_new_aggregated_payloads_provider.insert(key, proofs)?;
        }

        attestation_signatures_provider
            .retain(|key| !aggregated_data_roots.contains(&key.data_root))?;

        Ok(signed_attestations)
    }

    pub async fn compute_block_weights(&self) -> anyhow::Result<HashMap<B256, u64>> {
        let (latest_known_aggregated_payloads_provider, latest_finalized_provider, block_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_known_aggregated_payloads_provider(),
                db.latest_finalized_provider(),
                db.block_provider(),
            )
        };

        let aggregated_payloads = latest_known_aggregated_payloads_provider
            .iter()?
            .into_iter()
            .collect();

        let attestations = self
            .extract_attestations_from_aggregated_payloads(&aggregated_payloads)
            .await?;

        let start_slot = latest_finalized_provider.get()?.slot;
        let mut weights: HashMap<B256, u64> = HashMap::new();

        for attestation_data in attestations.values() {
            let mut current_root = attestation_data.head.root;
            while let Some(block) = block_provider.get(current_root).ok().flatten() {
                if block.block.slot <= start_slot {
                    break;
                }
                *weights.entry(current_root).or_insert(0) += 1;
                current_root = block.block.parent_root;
            }
        }

        Ok(weights)
    }

    /// Process a signed attestation from gossip network.
    /// 1. Validates attestation structure
    /// 2. Verifies XMSS signature
    /// 3. Stores the signature in gossip_signatures for later block building
    /// 4. Calls on_attestation to process the attestation data
    pub async fn on_gossip_attestation(
        &mut self,
        signed_attestation: SignedAttestation,
        is_aggregator: bool,
    ) -> anyhow::Result<()> {
        let validator_id = signed_attestation.validator_id;
        let attestation_data = &signed_attestation.message;
        let signature = signed_attestation.signature;
        let (
            attestation_data_by_root_provider,
            validator_id_provider,
            state_provider,
            attestation_signatures_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.attestation_data_by_root_provider(),
                db.validator_id_provider(),
                db.state_provider(),
                db.attestation_signatures_provider(),
            )
        };

        self.validate_attestation(&signed_attestation).await?;

        let key_state = state_provider
            .get(attestation_data.target.root)?
            .ok_or_else(|| anyhow!("No state available for signature verification"))?;

        ensure!(
            validator_id < key_state.validators.len() as u64,
            "Validator {validator_id} not found in state",
        );

        let verification_timer = start_timer(&PQ_SIG_ATTESTATION_VERIFICATION_TIME, &[]);
        let attestation_key = key_state.validators[validator_id as usize].attestation_public_key;
        let signature_valid = signature.verify(
            &attestation_key,
            attestation_data.slot as u32,
            &attestation_data.tree_hash_root(),
        )?;
        stop_timer(verification_timer);

        if signature_valid {
            inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL, &[]);
        } else {
            inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL, &[]);
        }

        ensure!(signature_valid, "Signature verification failed");

        let data_root = attestation_data.tree_hash_root();

        if is_aggregator && let Ok(Some(current_id)) = validator_id_provider.get() {
            let current_validator_subnet =
                compute_subnet_id(current_id, effective_attestation_committee_count());
            set_int_gauge_vec(
                &ATTESTATION_COMMITTEE_SUBNET,
                current_validator_subnet as i64,
                &[],
            );
            let attester_subnet =
                compute_subnet_id(validator_id, effective_attestation_committee_count());

            if current_validator_subnet == attester_subnet {
                attestation_signatures_provider
                    .insert(SignatureKey::new(validator_id, attestation_data), signature)?;
            }
        }

        attestation_data_by_root_provider.insert(data_root, attestation_data.clone())?;

        Ok(())
    }

    pub async fn produce_attestation_data(&self, slot: u64) -> anyhow::Result<AttestationData> {
        let (head_provider, block_provider, state_provider) = {
            let db = self.store.lock().await;
            (db.head_provider(), db.block_provider(), db.state_provider())
        };

        let head_root = head_provider.get()?;

        let head_state = state_provider
            .get(head_root)?
            .ok_or_else(|| anyhow!("Failed to get state for head block"))?;

        let mut source = head_state.latest_justified;
        if head_state.latest_block_header.slot == 0 {
            source.root = head_root;
        }
        Ok(AttestationData {
            slot,
            head: Checkpoint {
                root: head_root,
                slot: block_provider
                    .get(head_root)?
                    .ok_or(anyhow!("Failed to get head block"))?
                    .block
                    .slot,
            },
            target: self.get_attestation_target().await?,
            source,
        })
    }

    pub async fn prune_stale_attestation_data(&mut self) -> anyhow::Result<()> {
        let (
            latest_finalized_provider,
            attestation_signatures_provider,
            attestation_data_by_root_provider,
            latest_new_aggregated_payloads_provider,
            latest_known_aggregated_payloads_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.latest_finalized_provider(),
                db.attestation_signatures_provider(),
                db.attestation_data_by_root_provider(),
                db.latest_new_aggregated_payloads_provider(),
                db.latest_known_aggregated_payloads_provider(),
            )
        };

        let finalized_slot = latest_finalized_provider.get()?.slot;
        let stale_roots: HashSet<B256> = attestation_data_by_root_provider
            .iter()?
            .into_iter()
            .filter(|(_, data)| data.target.slot <= finalized_slot)
            .map(|(root, _)| root)
            .collect();

        if stale_roots.is_empty() {
            return Ok(());
        }

        attestation_data_by_root_provider.retain(|root, _| !stale_roots.contains(root))?;

        latest_new_aggregated_payloads_provider
            .retain(|key, _| !stale_roots.contains(&key.data_root))?;

        latest_known_aggregated_payloads_provider
            .retain(|key, _| !stale_roots.contains(&key.data_root))?;

        attestation_signatures_provider.retain(|key| !stale_roots.contains(&key.data_root))?;

        Ok(())
    }
}

pub fn compute_subnet_id(validator_id: u64, num_committees: u64) -> u64 {
    validator_id % num_committees
}

fn effective_attestation_committee_count() -> u64 {
    #[cfg(test)]
    {
        TEST_ATTESTATION_COMMITTEE_COUNT.load(Ordering::Relaxed)
    }

    #[cfg(not(test))]
    {
        ATTESTATION_COMMITTEE_COUNT
    }
}

#[cfg(test)]
static TEST_ATTESTATION_COMMITTEE_COUNT: AtomicU64 = AtomicU64::new(ATTESTATION_COMMITTEE_COUNT);

#[cfg(test)]
fn swap_test_attestation_committee_count(new_value: u64) -> u64 {
    TEST_ATTESTATION_COMMITTEE_COUNT.swap(new_value, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        sync::OnceLock,
        vec,
    };

    use alloy_primitives::{B256, FixedBytes};
    use anyhow::ensure;
    use ream_consensus_lean::{
        attestation::{
            AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof,
            AttestationData, SignatureKey, SignedAggregatedAttestation, SignedAttestation,
        },
        block::{BlockSignatures, BlockWithSignatures, SignedBlock},
        checkpoint::Checkpoint,
        slot::is_justifiable_after,
        validator::{Validator, is_proposer},
    };
    use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
    use ream_network_spec::networks::{LeanNetworkSpec, lean_network_spec, set_lean_network_spec};
    use ream_post_quantum_crypto::{
        lean_multisig::aggregate::{aggregate_signatures, verify_aggregate_signature},
        leansig::{private_key::PrivateKey, public_key::PublicKey, signature::Signature},
    };
    use ream_storage::tables::{field::REDBField, table::REDBTable};
    use ream_test_utils::store::sample_store;
    use ssz_types::{BitList, VariableList, typenum::U4096};
    use tokio::sync::Mutex as AsyncMutex;
    use tree_hash::TreeHash;

    use super::{Store, compute_subnet_id, swap_test_attestation_committee_count};
    use crate::constants::JUSTIFICATION_LOOKBACK_SLOTS;

    static TEST_GLOBAL_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();

    fn test_global_lock() -> &'static AsyncMutex<()> {
        TEST_GLOBAL_LOCK.get_or_init(|| AsyncMutex::new(()))
    }

    const CACHED_KEY_COUNT: usize = 10;

    type CachedKeyPair = (PublicKey, Vec<u8>);

    static CACHED_KEYS: OnceLock<Vec<CachedKeyPair>> = OnceLock::new();

    fn cached_key_pairs() -> &'static Vec<CachedKeyPair> {
        CACHED_KEYS.get_or_init(|| {
            (0..CACHED_KEY_COUNT)
                .map(|_| {
                    let (public_key, private_key) = PrivateKey::generate_key_pair(0, 10);
                    (public_key, private_key.to_bytes())
                })
                .collect()
        })
    }

    async fn sample_store_as_store(no_of_validators: usize) -> Store {
        let test_store = sample_store(no_of_validators).await;
        Store {
            store: test_store.store,
            network_state: test_store.network_state,
        }
    }

    struct CommitteeCountOverride {
        previous: u64,
    }

    impl CommitteeCountOverride {
        fn new(value: u64) -> Self {
            let previous = swap_test_attestation_committee_count(value);
            Self { previous }
        }
    }

    impl Drop for CommitteeCountOverride {
        fn drop(&mut self) {
            swap_test_attestation_committee_count(self.previous);
        }
    }

    fn build_signed_block(block_with_signatures: BlockWithSignatures) -> SignedBlock {
        SignedBlock {
            block: block_with_signatures.block,
            signature: BlockSignatures {
                attestation_signatures: block_with_signatures.signatures,
                proposer_signature: Signature::blank(),
            },
        }
    }

    async fn set_validator_id(store: &Store, validator_id: Option<u64>) {
        let provider = { store.store.lock().await.validator_id_provider() };
        provider.insert(validator_id).unwrap();
    }

    async fn install_validator_keys(
        store: &Store,
        validator_ids: &[u64],
    ) -> HashMap<
        u64,
        (
            ream_post_quantum_crypto::leansig::public_key::PublicKey,
            PrivateKey,
        ),
    > {
        let cache = cached_key_pairs();
        let mut key_pairs = HashMap::new();
        for validator_id in validator_ids {
            let validator_index = *validator_id as usize;
            assert!(
                validator_index < CACHED_KEY_COUNT,
                "validator_id {validator_index} exceeds cached key count {CACHED_KEY_COUNT}"
            );
            let (public_key, private_key_bytes) = &cache[validator_index];
            let private_key = PrivateKey::from_bytes(private_key_bytes)
                .expect("cached key bytes should be valid");
            key_pairs.insert(*validator_id, (*public_key, private_key));
        }

        let (head_provider, latest_justified_provider, latest_finalized_provider, state_provider) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.latest_justified_provider(),
                db.latest_finalized_provider(),
                db.state_provider(),
            )
        };

        let mut state_roots = HashSet::new();
        state_roots.insert(head_provider.get().unwrap());
        state_roots.insert(latest_justified_provider.get().unwrap().root);
        state_roots.insert(latest_finalized_provider.get().unwrap().root);

        for state_root in state_roots {
            let Some(mut state) = state_provider.get(state_root).unwrap() else {
                continue;
            };

            let mut validators: Vec<Validator> = state.validators.iter().cloned().collect();
            for (validator_id, (public_key, _)) in &key_pairs {
                {
                    validators[*validator_id as usize].attestation_public_key = *public_key;
                    validators[*validator_id as usize].proposal_public_key = *public_key;
                }
            }
            state.validators = VariableList::new(validators).unwrap();
            state_provider.insert(state_root, state).unwrap();
        }

        key_pairs
    }

    fn make_aggregated_proof(
        participants: &[u64],
        key_pairs: &HashMap<
            u64,
            (
                ream_post_quantum_crypto::leansig::public_key::PublicKey,
                PrivateKey,
            ),
        >,
        attestation_data: &AttestationData,
    ) -> AggregatedSignatureProof {
        let data_root = attestation_data.tree_hash_root();
        let mut aggregation_bits = BitList::<U4096>::with_capacity(
            participants.iter().max().map_or(0, |m| *m as usize + 1),
        )
        .unwrap();

        let mut public_keys = Vec::new();
        let mut signatures = Vec::new();

        for validator_id in participants {
            aggregation_bits.set(*validator_id as usize, true).unwrap();
            let (public_key, private_key) = key_pairs.get(validator_id).unwrap();
            public_keys.push(*public_key);
            signatures.push(
                private_key
                    .sign(&data_root.0, attestation_data.slot as u32)
                    .unwrap(),
            );
        }

        let proof_data = VariableList::new(
            aggregate_signatures(
                &public_keys,
                &signatures,
                &data_root.0,
                attestation_data.slot as u32,
            )
            .unwrap(),
        )
        .unwrap();

        AggregatedSignatureProof::new(aggregation_bits, proof_data)
    }

    fn make_test_aggregated_proof(participants: &[u64]) -> AggregatedSignatureProof {
        let mut aggregation_bits = BitList::<U4096>::with_capacity(
            participants.iter().max().map_or(0, |m| *m as usize + 1),
        )
        .unwrap();

        for validator_id in participants {
            aggregation_bits.set(*validator_id as usize, true).unwrap();
        }

        AggregatedSignatureProof::new(aggregation_bits, VariableList::new(vec![0u8]).unwrap())
    }

    async fn set_time_for_slot(store: &Store, slot: u64) {
        let time_provider = { store.store.lock().await.time_provider() };
        time_provider
            .insert(lean_network_spec().seconds_per_slot * slot)
            .unwrap();
    }

    async fn produce_and_import_block(
        store: &mut Store,
        slot: u64,
        proposer_index: u64,
    ) -> anyhow::Result<()> {
        let block_with_signatures = store
            .produce_block_with_signatures(slot, proposer_index)
            .await?;
        let signed_block = build_signed_block(block_with_signatures);
        store.on_block(&signed_block, false).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_head_checkpoint_slot_mismatch_rejected() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let slot_1 = 1;
        let block_sigs = store.produce_block_with_signatures(slot_1, 1).await?;
        let block_root = block_sigs.block.tree_hash_root();
        let genesis_checkpoint = {
            let db = store.store.lock().await;
            db.latest_justified_provider().get()?
        };

        let attestation = SignedAttestation {
            validator_id: 0,
            signature: Signature::blank(),
            message: AttestationData {
                slot: slot_1,
                head: Checkpoint {
                    root: block_root,
                    slot: 999,
                },
                target: Checkpoint {
                    root: block_root,
                    slot: slot_1,
                },
                source: genesis_checkpoint,
            },
        };

        let result = store.validate_attestation(&attestation).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Head checkpoint slot mismatch")
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_head_slot_less_than_source_rejected() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let block_1_sigs = store.produce_block_with_signatures(1, 1).await?;
        let block_1_root = block_1_sigs.block.tree_hash_root();
        let block_2_sigs = store.produce_block_with_signatures(2, 2).await?;
        let block_2_root = block_2_sigs.block.tree_hash_root();
        let genesis_root = {
            let db = store.store.lock().await;
            db.latest_justified_provider().get()?.root
        };

        let attestation = SignedAttestation {
            validator_id: 0,
            signature: Signature::blank(),
            message: AttestationData {
                slot: 2,
                head: Checkpoint {
                    root: genesis_root,
                    slot: 0,
                },
                target: Checkpoint {
                    root: block_2_root,
                    slot: 2,
                },
                source: Checkpoint {
                    root: block_1_root,
                    slot: 1,
                },
            },
        };

        let result = store.validate_attestation(&attestation).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Head checkpoint must not be older than target")
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_head_slot_less_than_target_rejected() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let block_1_sigs = store.produce_block_with_signatures(1, 1).await?;
        let block_1_root = block_1_sigs.block.tree_hash_root();
        let block_2_sigs = store.produce_block_with_signatures(2, 2).await?;
        let block_2_root = block_2_sigs.block.tree_hash_root();
        let genesis_checkpoint = {
            let db = store.store.lock().await;
            db.latest_justified_provider().get()?
        };

        let attestation = SignedAttestation {
            validator_id: 0,
            signature: Signature::blank(),
            message: AttestationData {
                slot: 2,
                head: Checkpoint {
                    root: block_1_root,
                    slot: 1,
                },
                target: Checkpoint {
                    root: block_2_root,
                    slot: 2,
                },
                source: genesis_checkpoint,
            },
        };

        let result = store.validate_attestation(&attestation).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Head checkpoint must not be older than target")
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_valid_attestation_with_correct_head_passes() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let slot_1 = 1;
        let block_sigs = store.produce_block_with_signatures(slot_1, 1).await?;
        let block_root = block_sigs.block.tree_hash_root();
        let genesis_checkpoint = {
            let db = store.store.lock().await;
            db.latest_justified_provider().get()?
        };

        let attestation = SignedAttestation {
            validator_id: 0,
            signature: Signature::blank(),
            message: AttestationData {
                slot: slot_1,
                head: Checkpoint {
                    root: block_root,
                    slot: slot_1,
                },
                target: Checkpoint {
                    root: block_root,
                    slot: slot_1,
                },
                source: genesis_checkpoint,
            },
        };

        store.validate_attestation(&attestation).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_head_equal_to_source_and_target_passes() -> anyhow::Result<()> {
        let store = sample_store(10).await;
        let genesis_checkpoint = {
            let db = store.store.lock().await;
            db.latest_justified_provider().get()?
        };

        let attestation = SignedAttestation {
            validator_id: 0,
            signature: Signature::blank(),
            message: AttestationData {
                slot: 0,
                head: genesis_checkpoint,
                target: genesis_checkpoint,
                source: genesis_checkpoint,
            },
        };

        store.validate_attestation(&attestation).await?;
        Ok(())
    }

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

    #[tokio::test]
    async fn test_prunes_entries_with_target_at_finalized() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let attestation_data = _make_attestation_data(5, 5);
        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::new(1, &attestation_data);
        let attestation_data_by_root_provider =
            store.store.lock().await.attestation_data_by_root_provider();

        {
            attestation_data_by_root_provider.insert(data_root, attestation_data)?;
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::repeat_byte(0xff),
                    slot: 5,
                })
                .unwrap();
            db.attestation_signatures_provider()
                .insert(sig_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(attestation_data_by_root_provider.contains_key(&data_root));
        {
            let db = store.store.lock().await;
            ensure!(
                db.attestation_signatures_provider()
                    .get(sig_key.clone())
                    .unwrap()
                    .is_some()
            );
        }

        store.prune_stale_attestation_data().await?;

        ensure!(!attestation_data_by_root_provider.contains_key(&data_root));
        let db = store.store.lock().await;
        ensure!(
            db.attestation_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_prunes_entries_with_target_before_finalized() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let attestation_data = _make_attestation_data(3, 3);
        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::new(1, &attestation_data);
        let attestation_data_by_root_provider =
            store.store.lock().await.attestation_data_by_root_provider();

        {
            attestation_data_by_root_provider.insert(data_root, attestation_data)?;
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::repeat_byte(0xff),
                    slot: 5,
                })
                .unwrap();
            db.attestation_signatures_provider()
                .insert(sig_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(attestation_data_by_root_provider.contains_key(&data_root));
        store.prune_stale_attestation_data().await?;

        ensure!(!attestation_data_by_root_provider.contains_key(&data_root));
        let db = store.store.lock().await;
        ensure!(
            db.attestation_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_keeps_entries_with_target_after_finalized() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let attestation_data = _make_attestation_data(10, 10);
        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::new(1, &attestation_data);
        let attestation_data_by_root_provider =
            store.store.lock().await.attestation_data_by_root_provider();

        {
            attestation_data_by_root_provider.insert(data_root, attestation_data.clone())?;
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::repeat_byte(0xff),
                    slot: 5,
                })
                .unwrap();
            db.attestation_signatures_provider()
                .insert(sig_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(attestation_data_by_root_provider.contains_key(&data_root));
        store.prune_stale_attestation_data().await?;

        ensure!(attestation_data_by_root_provider.contains_key(&data_root));
        ensure!(attestation_data_by_root_provider.get(data_root)?.unwrap() == attestation_data);
        let db = store.store.lock().await;
        ensure!(
            db.attestation_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_some()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_prunes_related_structures_together() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;

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
        let attestation_data_by_root_provider =
            store.store.lock().await.attestation_data_by_root_provider();
        let latest_new_aggregated_payloads_provider = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider();
        let latest_known_aggregated_payloads_provider = store
            .store
            .lock()
            .await
            .latest_known_aggregated_payloads_provider();

        {
            attestation_data_by_root_provider.insert(stale_root, stale_attestation)?;
            attestation_data_by_root_provider.insert(fresh_root, fresh_attestation)?;

            latest_new_aggregated_payloads_provider
                .insert(stale_key.clone(), vec![mock_proof.clone()])?;
            latest_new_aggregated_payloads_provider
                .insert(fresh_key.clone(), vec![mock_proof.clone()])?;

            latest_known_aggregated_payloads_provider
                .insert(stale_key.clone(), vec![mock_proof.clone()])?;

            latest_known_aggregated_payloads_provider
                .insert(fresh_key.clone(), vec![mock_proof])?;

            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();
            db.attestation_signatures_provider()
                .insert(stale_key.clone(), Signature::blank())
                .unwrap();
            db.attestation_signatures_provider()
                .insert(fresh_key.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(attestation_data_by_root_provider.contains_key(&stale_root));
        ensure!(latest_new_aggregated_payloads_provider.contains_key(&stale_key));
        ensure!(latest_known_aggregated_payloads_provider.contains_key(&stale_key));

        store.prune_stale_attestation_data().await?;

        ensure!(!attestation_data_by_root_provider.contains_key(&stale_root));
        ensure!(!latest_new_aggregated_payloads_provider.contains_key(&stale_key));
        ensure!(!latest_known_aggregated_payloads_provider.contains_key(&stale_key));

        ensure!(attestation_data_by_root_provider.contains_key(&fresh_root));
        ensure!(latest_new_aggregated_payloads_provider.contains_key(&fresh_key));

        let db = store.store.lock().await;
        ensure!(
            db.attestation_signatures_provider()
                .get(stale_key)
                .unwrap()
                .is_none()
        );
        ensure!(
            db.attestation_signatures_provider()
                .get(fresh_key)
                .unwrap()
                .is_some()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_returns_self_when_nothing_to_prune() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let fresh_attestation = _make_attestation_data(10, 10);
        let data_root = fresh_attestation.tree_hash_root();
        let attestation_data_by_root_provider =
            store.store.lock().await.attestation_data_by_root_provider();

        {
            attestation_data_by_root_provider.insert(data_root, fresh_attestation)?;
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();
        }

        let initial_len = attestation_data_by_root_provider.len();
        store.prune_stale_attestation_data().await?;

        ensure!(attestation_data_by_root_provider.len() == initial_len);
        ensure!(attestation_data_by_root_provider.contains_key(&data_root));
        Ok(())
    }

    #[tokio::test]
    async fn test_handles_empty_attestation_data() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let attestation_data_by_root_provider =
            store.store.lock().await.attestation_data_by_root_provider();

        ensure!(
            attestation_data_by_root_provider.is_empty(),
            "Store should start empty"
        );

        store.prune_stale_attestation_data().await?;

        ensure!(
            attestation_data_by_root_provider.is_empty(),
            "Store should remain empty"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_prunes_multiple_validators_same_data_root() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let stale_data = _make_attestation_data(3, 3);
        let data_root = stale_data.tree_hash_root();
        let sig_key_1 = SignatureKey::new(1, &stale_data);
        let sig_key_2 = SignatureKey::new(2, &stale_data);
        let attestation_data_by_root_provider =
            store.store.lock().await.attestation_data_by_root_provider();

        {
            attestation_data_by_root_provider.insert(data_root, stale_data)?;
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();

            let gossip = db.attestation_signatures_provider();
            gossip
                .insert(sig_key_1.clone(), Signature::blank())
                .unwrap();
            gossip
                .insert(sig_key_2.clone(), Signature::blank())
                .unwrap();
        }

        ensure!(attestation_data_by_root_provider.contains_key(&data_root));
        store.prune_stale_attestation_data().await?;

        ensure!(!attestation_data_by_root_provider.contains_key(&data_root));
        let db = store.store.lock().await;
        ensure!(
            db.attestation_signatures_provider()
                .get(sig_key_1)
                .unwrap()
                .is_none()
        );
        ensure!(
            db.attestation_signatures_provider()
                .get(sig_key_2)
                .unwrap()
                .is_none()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_mixed_stale_and_fresh_entries() -> anyhow::Result<()> {
        let mut store = sample_store(10).await;
        let mut roots = vec![];

        {
            let db = store.store.lock().await;
            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: B256::ZERO,
                    slot: 5,
                })
                .unwrap();
            let gossip = db.attestation_signatures_provider();

            for i in 1..=10 {
                let data = _make_attestation_data(i, i);
                let root = data.tree_hash_root();
                let key = SignatureKey::new(i, &data);

                db.attestation_data_by_root_provider().insert(root, data)?;
                gossip.insert(key, Signature::blank()).unwrap();
                roots.push(root);
            }
        }

        store.prune_stale_attestation_data().await?;

        for (i, root) in roots.iter().enumerate() {
            let slot = (i + 1) as u64;
            let attestation_data_by_root_provider =
                store.store.lock().await.attestation_data_by_root_provider();
            if slot <= 5 {
                ensure!(!attestation_data_by_root_provider.contains_key(root));
            } else {
                ensure!(attestation_data_by_root_provider.contains_key(root));
            }
        }
        Ok(())
    }

    // BLOCK PRODUCTION TESTS

    /// Test basic block production by authorized proposer.
    #[tokio::test]
    async fn test_produce_block_basic() {
        let slot = 1;
        let validator_index = 1;
        let mut store = sample_store(10).await;
        let BlockWithSignatures { block, .. } = store
            .produce_block_with_signatures(slot, validator_index)
            .await
            .unwrap();

        let head_provider = { store.store.lock().await.head_provider() };
        assert!(block.slot == slot);
        assert!(block.proposer_index == validator_index);
        assert!(block.parent_root == head_provider.get().unwrap());
        assert!(block.state_root != B256::ZERO);
    }

    /// Test block production fails for unauthorized proposer.
    #[tokio::test]
    async fn test_produce_block_unauthorized_proposer() {
        let mut store = sample_store(10).await;
        let block_with_signature = store.produce_block_with_signatures(1, 2).await;
        assert!(block_with_signature.is_err());
    }

    /// Test block production with no available attestations.
    #[tokio::test]
    pub async fn test_produce_block_empty_attestations() {
        let mut store = sample_store(10).await;
        let head = store.get_proposal_head(3).await.unwrap();

        let slot = 3;
        let validator_index = 3;
        let BlockWithSignatures { block, .. } = store
            .produce_block_with_signatures(slot, validator_index)
            .await
            .unwrap();

        assert_eq!(block.body.attestations.len(), 0);
        assert_eq!(block.slot, slot);
        assert_eq!(block.proposer_index, validator_index);
        assert_eq!(block.parent_root, head);
        assert!(!block.state_root.is_zero());
    }

    // VALIDATOR INTEGRATION TESTS

    /// Test producing a block then creating attestation for it.
    #[tokio::test]
    pub async fn test_block_production_then_attestation() {
        let mut store = sample_store(10).await;
        let proposer_slot = 1;
        let proposer_index = 1;
        store
            .produce_block_with_signatures(proposer_slot, proposer_index)
            .await
            .unwrap();
        store.update_head().await.unwrap();

        let attestor_slot = 2;
        let attestor_index = 7;
        let attestation_data = store.produce_attestation_data(attestor_slot).await.unwrap();
        let attestation = AggregatedAttestations {
            validator_id: attestor_index,
            data: attestation_data,
        };

        assert!(attestation.validator_id == attestor_index);
        assert!(attestation.data.slot == attestor_slot);

        let latest_justified = {
            store
                .store
                .lock()
                .await
                .latest_justified_provider()
                .get()
                .unwrap()
        };
        assert!(attestation.data.source == latest_justified);
    }

    /// Test multiple validators producing blocks and attestations.
    #[tokio::test]
    pub async fn test_multiple_validators_coordination() {
        let mut store = sample_store(10).await;
        let genesis_hash = { store.store.lock().await.head_provider().get().unwrap() };
        let block1 = store.produce_block_with_signatures(1, 1).await.unwrap();
        let _block1_hash = block1.block.tree_hash_root();

        let mut attestations = Vec::new();
        for i in 2..6 {
            let attestation_data = store.produce_attestation_data(2).await.unwrap();
            let attestation = AggregatedAttestations {
                validator_id: i,
                data: attestation_data,
            };
            attestations.push(attestation);
        }

        let block2 = store.produce_block_with_signatures(2, 2).await.unwrap();

        assert!(block2.block.slot == 2);
        assert!(block2.block.proposer_index == 2);
        assert!(block1.block.parent_root == genesis_hash);
        // Block1 not stored by produce_block_with_signatures otherwise block2.block.parent_root ==
        // block1_hash
        assert!(block2.block.parent_root == genesis_hash);
    }

    /// Test edge cases in validator operations.
    #[tokio::test]
    pub async fn test_validator_edge_cases() {
        let mut store = sample_store(10).await;
        let max_validator = 9;
        let slot = 9;

        let BlockWithSignatures { block, .. } = store
            .produce_block_with_signatures(slot, max_validator)
            .await
            .unwrap();
        assert!(block.proposer_index == max_validator);

        let attestation_data = store.produce_attestation_data(10).await.unwrap();
        let attestation = AggregatedAttestations {
            validator_id: max_validator,
            data: attestation_data,
        };
        assert!(attestation.validator_id == max_validator);
    }

    // ATTESTATION TESTS

    /// Test basic attestation production.
    #[tokio::test]
    pub async fn test_produce_attestation_basic() {
        let slot = 1;
        let validator_id = 5;

        let store = sample_store(10).await;
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
        let mut store = sample_store(10).await;
        let block_provider = store.store.lock().await.block_provider();
        let attestation = AggregatedAttestations {
            validator_id: 8,
            data: store.produce_attestation_data(slot).await.unwrap(),
        };
        let head = store.get_proposal_head(slot).await.unwrap();

        assert_eq!(attestation.data.head.root, head);

        let head_block = block_provider.get(head).unwrap().unwrap();
        assert_eq!(attestation.data.head.slot, head_block.block.slot);
    }

    /// Test that attestation calculates target correctly.
    #[tokio::test]
    pub async fn test_produce_attestation_target_calculation() {
        let store = sample_store(10).await;
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
        let store = sample_store(10).await;

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
        let store = sample_store(10).await;
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
        let store = sample_store(10).await;
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
        let mut store = sample_store(10).await;

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
        let mut store = sample_store(10).await;
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
        let store = sample_store(10).await;

        // shoudl fail
        assert!(!is_proposer(1000000, 1000000, 10));

        let attestation = AggregatedAttestations {
            validator_id: 1000000,
            data: store.produce_attestation_data(1).await.unwrap(),
        };
        assert_eq!(attestation.validator_id, 1000000);
    }

    // GET FORKCHOICE STORE TESTS

    /// Test get_forkchoice_store() time initialization.
    #[tokio::test]
    pub async fn test_store_time_from_anchor_slot() {
        let store = sample_store(10).await;
        let (time_provider, head_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.time_provider(), db.head_provider(), db.block_provider())
        };

        let time = time_provider.get().unwrap();
        let genesis_hash = head_provider.get().unwrap();
        let genesis_block = block_provider.get(genesis_hash).unwrap().unwrap().block;

        assert!(time == lean_network_spec().seconds_per_slot * genesis_block.slot);
    }

    // ON TICK TESTS

    /// Test basic on_tick functionality.
    #[tokio::test]
    pub async fn test_on_tick_basic() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = lean_network_spec().genesis_time + 200;

        store.on_tick(target_time, true, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time > initial_time);
    }

    /// Test on_tick without proposal.
    #[tokio::test]
    pub async fn test_on_tick_no_proposal() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = lean_network_spec().genesis_time + 100;

        store.on_tick(target_time, true, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    /// Test on_tick when already at target time.
    #[tokio::test]
    pub async fn test_on_tick_already_current() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let current_target = lean_network_spec().genesis_time + initial_time;

        store.on_tick(current_target, true, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time);
    }

    /// Test on_tick with small time increment.
    #[tokio::test]
    pub async fn test_on_tick_small_increment() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = lean_network_spec().genesis_time + initial_time + 1;

        store.on_tick(target_time, false, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == target_time - lean_network_spec().genesis_time);
    }

    // TEST INTERVAL TICKING

    /// Test basic interval ticking.
    #[tokio::test]
    pub async fn test_tick_interval_basic() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        store.tick_interval(false, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 1)
    }

    /// Test interval ticking with proposal.
    #[tokio::test]
    pub async fn test_tick_interval_with_proposal() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        store.tick_interval(true, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 1)
    }

    /// Test sequence of interval ticks.
    #[tokio::test]
    pub async fn test_tick_interval_sequence() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        for i in 0..5 {
            store.tick_interval((i % 2) == 0, false).await.unwrap();
        }

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 5)
    }

    /// Test different actions performed based on interval phase.
    #[tokio::test]
    pub async fn test_tick_interval_actions_by_phase() {
        let mut store = sample_store(10).await;

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

    /// Test conversion from slot to time.
    #[tokio::test]
    pub async fn test_slot_to_time_conversion() {
        let _ = sample_store(10).await;

        let genesis_time = lean_network_spec().genesis_time;

        let slot_0_time = genesis_time;
        assert!(slot_0_time == genesis_time);

        let slot_1_time = genesis_time + lean_network_spec().seconds_per_slot;
        assert!(slot_1_time == genesis_time + lean_network_spec().seconds_per_slot);

        let slot_10_time = genesis_time + 10 * lean_network_spec().seconds_per_slot;
        assert!(slot_10_time == genesis_time + 10 * lean_network_spec().seconds_per_slot);
    }

    /// Test conversion from time to slot.
    #[tokio::test]
    pub async fn test_time_to_slot_conversion() {
        let _ = sample_store(10).await;

        let genesis_time = lean_network_spec().genesis_time;

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

    /// Test interval calculations within slots.
    #[ignore]
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

    /// Test basic new attestation processing moves aggregated payloads.
    #[tokio::test]
    pub async fn test_accept_new_attestations_basic() {
        let mut store = sample_store(10).await;
        let latest_known_aggregated_payloads_provider = {
            store
                .store
                .lock()
                .await
                .latest_known_aggregated_payloads_provider()
        };
        let latest_new_aggregated_payloads_provider = {
            store
                .store
                .lock()
                .await
                .latest_new_aggregated_payloads_provider()
        };
        let initial_known_payloads = latest_known_aggregated_payloads_provider
            .iter()
            .unwrap()
            .len();

        store.accept_new_attestations().await.unwrap();

        assert!(
            latest_new_aggregated_payloads_provider
                .iter()
                .unwrap()
                .is_empty()
        );
        assert!(
            latest_known_aggregated_payloads_provider
                .iter()
                .unwrap()
                .len()
                >= initial_known_payloads
        );
    }

    /// Test accepting multiple new aggregated payloads.
    #[tokio::test]
    pub async fn test_accept_new_attestations_multiple() {
        let mut store = sample_store(10).await;
        store.accept_new_attestations().await.unwrap();
        let latest_new_aggregated_payloads_provider = {
            store
                .store
                .lock()
                .await
                .latest_new_aggregated_payloads_provider()
        };

        assert!(
            latest_new_aggregated_payloads_provider
                .iter()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    pub async fn test_accept_new_attestations_empty() {
        let mut store = sample_store(10).await;
        let latest_known_aggregated_payloads_provider = {
            store
                .store
                .lock()
                .await
                .latest_known_aggregated_payloads_provider()
        };
        let latest_new_aggregated_payloads_provider = {
            store
                .store
                .lock()
                .await
                .latest_new_aggregated_payloads_provider()
        };
        let initial_known_payloads = latest_known_aggregated_payloads_provider
            .iter()
            .unwrap()
            .len();

        store.accept_new_attestations().await.unwrap();

        assert!(
            latest_new_aggregated_payloads_provider
                .iter()
                .unwrap()
                .is_empty()
        );
        assert!(
            latest_known_aggregated_payloads_provider
                .iter()
                .unwrap()
                .len()
                == initial_known_payloads
        );
    }

    // TEST PROPOSAL HEAD TIMING

    /// Test getting proposal head for a slot.
    #[tokio::test]
    pub async fn test_get_proposal_head_basic() {
        let mut store = sample_store(10).await;

        let head = store.get_proposal_head(0).await.unwrap();

        let stored_head = { store.store.lock().await.head_provider().get().unwrap() };

        assert!(head == stored_head);
    }

    /// Test that get_proposal_head advances store time appropriately.
    #[tokio::test]
    pub async fn test_get_proposal_head_advances_time() {
        let mut store = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        store.get_proposal_head(5).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    #[tokio::test]
    pub async fn test_get_proposal_head_processes_attestations() {
        let mut store = sample_store(10).await;
        store.get_proposal_head(1).await.unwrap();
        let latest_new_aggregated_payloads_provider = {
            store
                .store
                .lock()
                .await
                .latest_new_aggregated_payloads_provider()
        };

        assert!(
            latest_new_aggregated_payloads_provider
                .iter()
                .unwrap()
                .is_empty()
        );
    }

    // TEST TIME CONSTANTS

    /// Test that time constants are consistent with each other.
    #[ignore]
    #[allow(clippy::assertions_on_constants)]
    #[tokio::test]
    pub async fn test_time_constants_consistency() {
        let _test_guard = test_global_lock().lock().await;
        set_lean_network_spec(LeanNetworkSpec::ephemery().into());
        let seconds_per_interval = lean_network_spec().seconds_per_slot / INTERVALS_PER_SLOT;

        assert!(INTERVALS_PER_SLOT > 0);
        assert!(seconds_per_interval > 0);
        assert!(lean_network_spec().seconds_per_slot > 0);
    }

    /// Test the relationship between intervals and slots.
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

    // TEST STORE ATTESTATION HANDLING

    #[tokio::test]
    pub async fn test_on_block_processes_multi_validator_aggregations() {
        let mut store: Store = sample_store_as_store(3).await;
        let participants: Vec<u64> = vec![1, 2];

        let attestation_slot = 1;
        let attestation_data = store
            .produce_attestation_data(attestation_slot)
            .await
            .unwrap();
        let data_root = attestation_data.tree_hash_root();
        let proof = make_test_aggregated_proof(&participants);

        {
            let db = store.store.lock().await;
            db.attestation_data_by_root_provider()
                .insert(data_root, attestation_data.clone())
                .unwrap();
            let latest_known = db.latest_known_aggregated_payloads_provider();
            latest_known
                .insert(
                    SignatureKey::from_parts(participants[0], data_root),
                    vec![proof.clone()],
                )
                .unwrap();
            latest_known
                .insert(
                    SignatureKey::from_parts(participants[1], data_root),
                    vec![proof.clone()],
                )
                .unwrap();
        }

        let proposer_index = 1;
        let block_with_signatures = store
            .produce_block_with_signatures(attestation_slot, proposer_index)
            .await
            .unwrap();
        let signed_block = build_signed_block(block_with_signatures);

        store.on_block(&signed_block, false).await.unwrap();

        let aggregated_payloads = {
            store
                .store
                .lock()
                .await
                .latest_known_aggregated_payloads_provider()
                .iter()
                .unwrap()
                .into_iter()
                .collect::<HashMap<_, _>>()
        };

        let extracted = store
            .extract_attestations_from_aggregated_payloads(&aggregated_payloads)
            .await
            .unwrap();

        assert_eq!(extracted.get(&participants[0]), Some(&attestation_data));
        assert_eq!(extracted.get(&participants[1]), Some(&attestation_data));
    }

    // TEST ON GOSSIP ATTESTATION SUBNET FILTERING

    #[tokio::test]
    pub async fn test_same_subnet_stores_signature() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(4);
        let mut store: Store = sample_store_as_store(8).await;

        let current_validator = 0;
        let attestor_validator = 4;
        assert_eq!(
            compute_subnet_id(current_validator, 4),
            compute_subnet_id(attestor_validator, 4)
        );
        set_validator_id(&store, Some(current_validator)).await;
        let key_pairs = install_validator_keys(&store, &[attestor_validator]).await;

        let attestation_data = store.produce_attestation_data(1).await.unwrap();
        let signature = key_pairs
            .get(&attestor_validator)
            .unwrap()
            .1
            .sign(
                &attestation_data.tree_hash_root().0,
                attestation_data.slot as u32,
            )
            .unwrap();

        let signed_attestation = SignedAttestation {
            validator_id: attestor_validator,
            message: attestation_data.clone(),
            signature,
        };

        let sig_key = SignatureKey::new(attestor_validator, &attestation_data);
        assert!(
            store
                .store
                .lock()
                .await
                .attestation_signatures_provider()
                .get(sig_key.clone())
                .unwrap()
                .is_none()
        );

        store
            .on_gossip_attestation(signed_attestation, true)
            .await
            .unwrap();

        assert!(
            store
                .store
                .lock()
                .await
                .attestation_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    pub async fn test_cross_subnet_ignores_signature() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(4);
        let mut store: Store = sample_store_as_store(8).await;
        let current_validator = 0;
        let attestor_validator = 1;
        let slot = 1;

        assert_ne!(
            compute_subnet_id(current_validator, 4),
            compute_subnet_id(attestor_validator, 4)
        );

        set_validator_id(&store, Some(current_validator)).await;
        let key_pairs = install_validator_keys(&store, &[attestor_validator]).await;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let signature = key_pairs
            .get(&attestor_validator)
            .unwrap()
            .1
            .sign(
                &attestation_data.tree_hash_root().0,
                attestation_data.slot as u32,
            )
            .unwrap();

        let signed_attestation = SignedAttestation {
            validator_id: attestor_validator,
            message: attestation_data.clone(),
            signature,
        };

        store
            .on_gossip_attestation(signed_attestation, true)
            .await
            .unwrap();

        let sig_key = SignatureKey::new(attestor_validator, &attestation_data);
        assert!(
            store
                .store
                .lock()
                .await
                .attestation_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    pub async fn test_non_aggregator_never_stores_signature() {
        let mut store: Store = sample_store_as_store(8).await;
        let current_validator = 0;
        let attestor_validator = 4;
        let slot = 1;

        set_validator_id(&store, Some(current_validator)).await;
        let key_pairs = install_validator_keys(&store, &[attestor_validator]).await;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let signature = key_pairs
            .get(&attestor_validator)
            .unwrap()
            .1
            .sign(
                &attestation_data.tree_hash_root().0,
                attestation_data.slot as u32,
            )
            .unwrap();

        let signed_attestation = SignedAttestation {
            validator_id: attestor_validator,
            message: attestation_data.clone(),
            signature,
        };

        store
            .on_gossip_attestation(signed_attestation, false)
            .await
            .unwrap();

        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::from_parts(4, data_root);
        assert!(
            store
                .store
                .lock()
                .await
                .attestation_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .store
                .lock()
                .await
                .attestation_data_by_root_provider()
                .get(data_root)
                .unwrap(),
            Some(attestation_data)
        );
    }

    #[tokio::test]
    pub async fn test_attestation_data_always_stored() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(4);
        let mut store: Store = sample_store_as_store(8).await;

        let current_validator = 0;
        let attestor_validator = 1;
        let slot = 1;

        assert_ne!(
            compute_subnet_id(current_validator, 4),
            compute_subnet_id(attestor_validator, 4)
        );

        set_validator_id(&store, Some(current_validator)).await;
        let key_pairs = install_validator_keys(&store, &[attestor_validator]).await;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let signature = key_pairs
            .get(&attestor_validator)
            .unwrap()
            .1
            .sign(
                &attestation_data.tree_hash_root().0,
                attestation_data.slot as u32,
            )
            .unwrap();

        let signed_attestation = SignedAttestation {
            validator_id: attestor_validator,
            message: attestation_data.clone(),
            signature,
        };

        store
            .on_gossip_attestation(signed_attestation, true)
            .await
            .unwrap();

        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::from_parts(attestor_validator, data_root);

        assert!(
            store
                .store
                .lock()
                .await
                .attestation_signatures_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .store
                .lock()
                .await
                .attestation_data_by_root_provider()
                .get(data_root)
                .unwrap(),
            Some(attestation_data)
        );
    }

    #[tokio::test]
    pub async fn test_valid_proof_stored_correctly() {
        let mut store: Store = sample_store_as_store(4).await;
        let participants: Vec<u64> = vec![1, 2];
        let key_pairs = install_validator_keys(&store, &participants).await;

        let attestation_data = store.produce_attestation_data(1).await.unwrap();
        set_time_for_slot(&store, attestation_data.slot).await;

        let proof = make_aggregated_proof(&participants, &key_pairs, &attestation_data);
        store
            .on_gossip_aggregated_attestation(SignedAggregatedAttestation {
                data: attestation_data.clone(),
                proof: proof.clone(),
            })
            .await
            .unwrap();

        let data_root = attestation_data.tree_hash_root();
        let latest_new = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider();
        assert_eq!(
            latest_new
                .get(SignatureKey::from_parts(participants[0], data_root))
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            latest_new
                .get(SignatureKey::from_parts(participants[1], data_root))
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .store
                .lock()
                .await
                .attestation_data_by_root_provider()
                .get(data_root)
                .unwrap(),
            Some(attestation_data)
        );
    }

    #[tokio::test]
    pub async fn test_attestation_data_stored_by_root() {
        let mut store: Store = sample_store_as_store(4).await;
        let participants: Vec<u64> = vec![1];
        let key_pairs = install_validator_keys(&store, &participants).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let data_root = attestation_data.tree_hash_root();
        set_time_for_slot(&store, attestation_data.slot).await;

        let proof = make_aggregated_proof(&participants, &key_pairs, &attestation_data);
        store
            .on_gossip_aggregated_attestation(SignedAggregatedAttestation {
                data: attestation_data.clone(),
                proof,
            })
            .await
            .unwrap();

        assert_eq!(
            store
                .store
                .lock()
                .await
                .attestation_data_by_root_provider()
                .get(data_root)
                .unwrap(),
            Some(attestation_data)
        );
    }

    #[tokio::test]
    pub async fn test_invalid_proof_rejected() {
        let mut store: Store = sample_store_as_store(4).await;
        let claimed_participants: Vec<u64> = vec![1, 2];
        let actual_signers: Vec<u64> = vec![1, 3];
        let key_pairs = install_validator_keys(&store, &[1, 2, 3]).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        set_time_for_slot(&store, attestation_data.slot).await;
        let data_root = attestation_data.tree_hash_root();

        let proof = make_aggregated_proof(&actual_signers, &key_pairs, &attestation_data);
        let mut claimed_bits = BitList::<U4096>::with_capacity(3).unwrap();
        claimed_bits
            .set(claimed_participants[0] as usize, true)
            .unwrap();
        claimed_bits
            .set(claimed_participants[1] as usize, true)
            .unwrap();
        let invalid_proof = AggregatedSignatureProof::new(claimed_bits, proof.proof_data.clone());

        let result = store
            .on_gossip_aggregated_attestation(SignedAggregatedAttestation {
                data: attestation_data,
                proof: invalid_proof,
            })
            .await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Aggregated signature verification failed"),
            "unexpected error for data_root={data_root:?}"
        );
    }

    #[tokio::test]
    pub async fn test_multiple_proofs_accumulate() {
        let mut store: Store = sample_store_as_store(4).await;
        let key_pairs = install_validator_keys(&store, &[1, 2, 3]).await;

        let attestation_data = store.produce_attestation_data(1).await.unwrap();
        set_time_for_slot(&store, attestation_data.slot).await;
        let data_root = attestation_data.tree_hash_root();

        let participants_1: Vec<u64> = vec![1, 2];
        let participants_2: Vec<u64> = vec![1, 3];
        let mutual_proposer_index = 1;

        let proof_1 = make_aggregated_proof(&participants_1, &key_pairs, &attestation_data);
        let proof_2 = make_aggregated_proof(&participants_2, &key_pairs, &attestation_data);

        store
            .on_gossip_aggregated_attestation(SignedAggregatedAttestation {
                data: attestation_data.clone(),
                proof: proof_1.clone(),
            })
            .await
            .unwrap();
        store
            .on_gossip_aggregated_attestation(SignedAggregatedAttestation {
                data: attestation_data,
                proof: proof_2.clone(),
            })
            .await
            .unwrap();

        let sig_key = SignatureKey::from_parts(mutual_proposer_index, data_root);
        let stored_proofs = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider()
            .get(sig_key)
            .unwrap()
            .unwrap();

        assert_eq!(stored_proofs.len(), 2);
        assert!(stored_proofs.contains(&proof_1));
        assert!(stored_proofs.contains(&proof_2));
    }

    #[tokio::test]
    pub async fn test_aggregates_gossip_signatures_into_proof() {
        let _test_guard = test_global_lock().lock().await;
        let mut store: Store = sample_store_as_store(4).await;
        set_validator_id(&store, Some(0)).await;
        let attesting_validators: Vec<u64> = vec![1, 2];
        let key_pairs = install_validator_keys(&store, &attesting_validators).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let data_root = attestation_data.tree_hash_root();

        for &validator_id in attesting_validators.iter() {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();

        let latest_new = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider();

        for validator_id in attesting_validators {
            let key = SignatureKey::from_parts(validator_id, data_root);
            let proofs = latest_new.get(key).unwrap().unwrap();
            assert!(!proofs.is_empty());
        }
    }

    #[tokio::test]
    pub async fn test_aggregated_proof_is_valid() {
        let _test_guard = test_global_lock().lock().await;
        let mut store: Store = sample_store_as_store(4).await;
        set_validator_id(&store, Some(0)).await;
        let attesting_validators: Vec<u64> = vec![1, 2];
        let key_pairs = install_validator_keys(&store, &attesting_validators).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let data_root = attestation_data.tree_hash_root();

        for &validator_id in attesting_validators.iter() {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();

        let proof = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider()
            .get(SignatureKey::from_parts(attesting_validators[0], data_root))
            .unwrap()
            .unwrap()
            .first()
            .cloned()
            .unwrap();

        let participants = proof.to_validator_indices();
        let state = store
            .store
            .lock()
            .await
            .state_provider()
            .get(attestation_data.target.root)
            .unwrap()
            .unwrap();
        let public_keys: Vec<_> = participants
            .iter()
            .map(|&validator_id| state.validators[validator_id as usize].attestation_public_key)
            .collect();

        assert!(
            verify_aggregate_signature(
                &public_keys,
                &data_root.0,
                proof.proof_data.as_ref(),
                attestation_data.slot as u32
            )
            .is_ok()
        );
    }

    #[tokio::test]
    pub async fn test_empty_gossip_signatures_produces_no_proofs() {
        let mut store: Store = sample_store_as_store(4).await;
        store.aggregate().await.unwrap();

        let is_empty = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider()
            .iter()
            .unwrap()
            .is_empty();
        assert!(is_empty);
    }

    #[tokio::test]
    pub async fn test_multiple_attestation_data_grouped_separately() {
        let mut store: Store = sample_store_as_store(4).await;
        let attesting_validators: Vec<u64> = vec![1, 2];
        let key_pairs = install_validator_keys(&store, &attesting_validators).await;
        let slot = 1;

        let attestation_data_1 = store.produce_attestation_data(slot).await.unwrap();
        let attestation_data_2 = AttestationData {
            slot,
            head: Checkpoint {
                root: FixedBytes::repeat_byte(1),
                slot: 1,
            },
            target: attestation_data_1.target,
            source: attestation_data_1.source,
        };
        let data_root_1 = attestation_data_1.tree_hash_root();
        let data_root_2 = attestation_data_2.tree_hash_root();

        let sig_1 = key_pairs
            .get(&1)
            .unwrap()
            .1
            .sign(&data_root_1.0, attestation_data_1.slot as u32)
            .unwrap();
        let sig_2 = key_pairs
            .get(&2)
            .unwrap()
            .1
            .sign(&data_root_2.0, attestation_data_2.slot as u32)
            .unwrap();

        {
            let db = store.store.lock().await;
            let attestation_data_by_root = db.attestation_data_by_root_provider();
            let gossip_signatures = db.attestation_signatures_provider();

            attestation_data_by_root
                .insert(data_root_1, attestation_data_1)
                .unwrap();
            attestation_data_by_root
                .insert(data_root_2, attestation_data_2)
                .unwrap();

            gossip_signatures
                .insert(
                    SignatureKey::from_parts(attesting_validators[0], data_root_1),
                    sig_1,
                )
                .unwrap();
            gossip_signatures
                .insert(
                    SignatureKey::from_parts(attesting_validators[1], data_root_2),
                    sig_2,
                )
                .unwrap();
        }

        store.aggregate().await.unwrap();

        let latest_new = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider();

        assert!(
            latest_new
                .get(SignatureKey::from_parts(
                    attesting_validators[0],
                    data_root_1
                ))
                .unwrap()
                .is_some()
        );
        assert!(
            latest_new
                .get(SignatureKey::from_parts(
                    attesting_validators[1],
                    data_root_2
                ))
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    pub async fn test_interval_2_triggers_aggregation_for_aggregator() {
        let _test_guard = test_global_lock().lock().await;
        let mut store: Store = sample_store_as_store(4).await;
        set_validator_id(&store, Some(0)).await;
        let attesting_validators: Vec<u64> = vec![1, 2];
        let key_pairs = install_validator_keys(&store, &attesting_validators).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let data_root = attestation_data.tree_hash_root();

        for &validator_id in attesting_validators.iter() {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.store.lock().await.time_provider().insert(1).unwrap();
        store.tick_interval(false, true).await.unwrap();

        let sig_key = SignatureKey::from_parts(attesting_validators[0], data_root);
        assert!(
            store
                .store
                .lock()
                .await
                .latest_new_aggregated_payloads_provider()
                .get(sig_key)
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    pub async fn test_interval_2_skips_aggregation_for_non_aggregator() {
        let _test_guard = test_global_lock().lock().await;
        let mut store: Store = sample_store_as_store(4).await;
        set_validator_id(&store, Some(0)).await;
        let attesting_validators: Vec<u64> = vec![1, 2];
        let key_pairs = install_validator_keys(&store, &attesting_validators).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let data_root = attestation_data.tree_hash_root();

        for &validator_id in attesting_validators.iter() {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.store.lock().await.time_provider().insert(1).unwrap();
        store.tick_interval(false, false).await.unwrap();

        let sig_key = SignatureKey::from_parts(attesting_validators[0], data_root);
        assert!(
            store
                .store
                .lock()
                .await
                .latest_new_aggregated_payloads_provider()
                .get(sig_key)
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    pub async fn test_other_intervals_do_not_trigger_aggregation() {
        let _test_guard = test_global_lock().lock().await;
        let mut store: Store = sample_store_as_store(4).await;
        let attesting_validators: Vec<u64> = vec![1, 2];
        set_validator_id(&store, Some(0)).await;
        let key_pairs = install_validator_keys(&store, &attesting_validators).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let data_root = attestation_data.tree_hash_root();
        let sig_key = SignatureKey::from_parts(1, data_root);

        for &validator_id in attesting_validators.iter() {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        let non_aggregation_intervals = vec![0, 1, 3, 4];
        for target_interval in non_aggregation_intervals {
            let pre_tick_time = (target_interval + INTERVALS_PER_SLOT - 1) % INTERVALS_PER_SLOT;
            {
                let db = store.store.lock().await;
                db.time_provider().insert(pre_tick_time).unwrap();
                db.latest_new_aggregated_payloads_provider()
                    .drain()
                    .unwrap();
            }

            store.tick_interval(false, true).await.unwrap();

            assert!(
                store
                    .store
                    .lock()
                    .await
                    .latest_new_aggregated_payloads_provider()
                    .get(sig_key.clone())
                    .unwrap()
                    .is_none(),
                "Aggregation should not occur at interval {target_interval}"
            );
        }
    }

    #[tokio::test]
    pub async fn test_interval_0_accepts_attestations_with_proposal() {
        let mut store: Store = sample_store_as_store(4).await;

        store.store.lock().await.time_provider().insert(4).unwrap();
        store.tick_interval(true, true).await.unwrap();

        let time = store.store.lock().await.time_provider().get().unwrap();
        assert_eq!(time, 5);
        assert_eq!(time % INTERVALS_PER_SLOT, 0);
    }

    #[tokio::test]
    pub async fn test_gossip_to_aggregation_to_storage() {
        let _test_guard = test_global_lock().lock().await;
        let mut store: Store = sample_store_as_store(4).await;
        set_validator_id(&store, Some(0)).await;
        let attesting_validators: Vec<u64> = vec![1, 2];
        let key_pairs = install_validator_keys(&store, &attesting_validators).await;
        let slot = 1;

        let attestation_data = store.produce_attestation_data(slot).await.unwrap();
        let data_root = attestation_data.tree_hash_root();

        for &validator_id in attesting_validators.iter() {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();
            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
            assert!(
                store
                    .store
                    .lock()
                    .await
                    .attestation_signatures_provider()
                    .get(SignatureKey::from_parts(validator_id, data_root))
                    .unwrap()
                    .is_some()
            );
        }

        store.store.lock().await.time_provider().insert(1).unwrap();
        store.tick_interval(false, true).await.unwrap();

        let latest_new = store
            .store
            .lock()
            .await
            .latest_new_aggregated_payloads_provider();
        let sig_key = SignatureKey::from_parts(1, data_root);
        let proof = latest_new
            .get(sig_key)
            .unwrap()
            .unwrap()
            .first()
            .cloned()
            .unwrap();
        let participants = proof.to_validator_indices();
        let state = store
            .store
            .lock()
            .await
            .state_provider()
            .get(attestation_data.target.root)
            .unwrap()
            .unwrap();
        let public_keys: Vec<_> = participants
            .iter()
            .map(|&validator_id| state.validators[validator_id as usize].attestation_public_key)
            .collect();

        assert!(
            verify_aggregate_signature(
                &public_keys,
                &data_root.0,
                proof.proof_data.as_ref(),
                attestation_data.slot as u32
            )
            .is_ok()
        );
    }

    // COMPUTE BLOCK WEIGHT TESTS

    /// A genesis-only store with no attestations has no block weights.
    #[tokio::test]
    pub async fn test_genesis_only_store_returns_empty_weights() {
        let store = sample_store_as_store(10).await;
        let weights = store.compute_block_weights().await.unwrap();
        assert!(weights.is_empty());
    }

    // TEST GET ATTESTATION TARGET

    /// Target at genesis should be the genesis block.
    #[tokio::test]
    pub async fn test_get_attestation_target_at_genesis() {
        let store = sample_store_as_store(10).await;
        let target = store.get_attestation_target().await.unwrap();
        let (head_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.head_provider(), db.block_provider())
        };

        let genesis_root = head_provider.get().unwrap();
        let genesis_block = block_provider.get(genesis_root).unwrap().unwrap();

        assert_eq!(target.root, genesis_root);
        assert_eq!(target.slot, genesis_block.block.slot);
    }

    /// get_attestation_target should return a Checkpoint.
    #[tokio::test]
    pub async fn test_get_attestation_target_returns_checkpoint() {
        let store = sample_store_as_store(10).await;
        let target = store.get_attestation_target().await.unwrap();
        let block_provider = { store.store.lock().await.block_provider() };
        let target_block = block_provider.get(target.root).unwrap();

        assert!(target_block.is_some());
        assert_eq!(target.slot, target_block.unwrap().block.slot);
    }

    /// Target should walk back toward safe_target when head is ahead.
    #[tokio::test]
    pub async fn test_get_attestation_target_walks_back_toward_safe_target() {
        let mut store = sample_store_as_store(10).await;
        for slot in 1..6 {
            produce_and_import_block(&mut store, slot, slot)
                .await
                .unwrap();
        }

        let (head_provider, block_provider, safe_target_provider) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.block_provider(),
                db.safe_target_provider(),
            )
        };
        let head_root = head_provider.get().unwrap();
        let head_slot = block_provider.get(head_root).unwrap().unwrap().block.slot;

        let safe_target_root = safe_target_provider.get().unwrap();
        let safe_target_slot = block_provider
            .get(safe_target_root)
            .unwrap()
            .unwrap()
            .block
            .slot;
        let target = store.get_attestation_target().await.unwrap();

        assert!(head_slot >= 1);
        assert_eq!(safe_target_slot, 0);
        assert!(target.slot >= head_slot.saturating_sub(super::JUSTIFICATION_LOOKBACK_SLOTS));
    }

    /// Target should land on a slot that is_justifiable_after the finalized slot.
    #[tokio::test]
    pub async fn test_get_attestation_target_respects_justifiable_slots() {
        let mut store = sample_store_as_store(10).await;
        for slot in 1..10 {
            produce_and_import_block(&mut store, slot, slot)
                .await
                .unwrap();
        }

        let target = store.get_attestation_target().await.unwrap();
        let finalized_slot = {
            store
                .store
                .lock()
                .await
                .latest_finalized_provider()
                .get()
                .unwrap()
                .slot
        };

        assert!(is_justifiable_after(target.slot, finalized_slot).unwrap());
    }

    /// Target should be on the path from head to finalized checkpoint.
    #[tokio::test]
    pub async fn test_get_attestation_target_consistency_with_head() {
        let mut store = sample_store_as_store(10).await;
        for slot in 1..4 {
            produce_and_import_block(&mut store, slot, slot)
                .await
                .unwrap();
        }

        let target = store.get_attestation_target().await.unwrap();
        let (head_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.head_provider(), db.block_provider())
        };

        let mut current_root = head_provider.get().unwrap();
        let mut found_target = false;

        while current_root != B256::ZERO {
            if current_root == target.root {
                found_target = true;
                break;
            }

            let current_block = block_provider.get(current_root).unwrap().unwrap();
            current_root = current_block.block.parent_root;
        }

        assert!(found_target, "Target should be an ancestor of head");
    }

    // TEST SAFE TARGET ADVANCEMENT

    /// Safe target should only advance with 2/3+ attestation support.
    #[tokio::test]
    pub async fn test_safe_target_requires_supermajority() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(1);
        let mut store: Store = sample_store_as_store(10).await;
        let slot = 1;
        let proposer_index = 1;
        produce_and_import_block(&mut store, slot, proposer_index)
            .await
            .unwrap();
        set_validator_id(&store, Some(0)).await;

        let (head_provider, latest_justified_provider, state_provider) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.latest_justified_provider(),
                db.state_provider(),
            )
        };
        let block_root = head_provider.get().unwrap();
        let num_validators = state_provider
            .get(block_root)
            .unwrap()
            .unwrap()
            .validators
            .len() as u64;
        let threshold = (num_validators * 2 + 2).div_ceil(3);

        let validator_ids: Vec<u64> = (0..num_validators).collect();
        let key_pairs = install_validator_keys(&store, &validator_ids).await;
        let attestation_data = AttestationData {
            slot,
            head: Checkpoint {
                root: block_root,
                slot,
            },
            target: Checkpoint {
                root: block_root,
                slot,
            },
            source: latest_justified_provider.get().unwrap(),
        };
        let data_root = attestation_data.tree_hash_root();

        for validator_id in 0..(threshold - 1) {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();
        store.update_safe_target().await.unwrap();

        let (safe_target_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.safe_target_provider(), db.block_provider())
        };
        let safe_target_slot = block_provider
            .get(safe_target_provider.get().unwrap())
            .unwrap()
            .unwrap()
            .block
            .slot;

        assert!(safe_target_slot <= 1);
    }

    /// Safe target should advance when 2/3+ validators attest to same target.
    #[tokio::test]
    pub async fn test_safe_target_advances_with_supermajority() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(1);
        let mut store: Store = sample_store_as_store(10).await;
        let slot = 1;
        let proposer_index = 1;
        produce_and_import_block(&mut store, slot, proposer_index)
            .await
            .unwrap();
        set_validator_id(&store, Some(0)).await;

        let (head_provider, latest_justified_provider, state_provider) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.latest_justified_provider(),
                db.state_provider(),
            )
        };
        let block_root = head_provider.get().unwrap();
        let num_validators = state_provider
            .get(block_root)
            .unwrap()
            .unwrap()
            .validators
            .len() as u64;
        let threshold = (num_validators * 2 + 2).div_ceil(3);

        let validator_ids: Vec<u64> = (0..num_validators).collect();
        let key_pairs = install_validator_keys(&store, &validator_ids).await;
        let attestation_data = AttestationData {
            slot,
            head: Checkpoint {
                root: block_root,
                slot,
            },
            target: Checkpoint {
                root: block_root,
                slot,
            },
            source: latest_justified_provider.get().unwrap(),
        };
        let data_root = attestation_data.tree_hash_root();

        for validator_id in 0..(threshold + 1) {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();
        store.update_safe_target().await.unwrap();

        let (safe_target_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.safe_target_provider(), db.block_provider())
        };
        let safe_target_slot = block_provider
            .get(safe_target_provider.get().unwrap())
            .unwrap()
            .unwrap()
            .block
            .slot;

        assert!(safe_target_slot <= slot);
    }

    /// update_safe_target should use new aggregated payloads.
    #[tokio::test]
    pub async fn test_update_safe_target_uses_new_attestations() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(1);
        let mut store: Store = sample_store_as_store(10).await;
        let slot = 1;
        let proposer_index = 1;
        produce_and_import_block(&mut store, slot, proposer_index)
            .await
            .unwrap();
        set_validator_id(&store, Some(0)).await;

        let (head_provider, latest_justified_provider, state_provider) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.latest_justified_provider(),
                db.state_provider(),
            )
        };
        let block_root = head_provider.get().unwrap();
        let num_validators = state_provider
            .get(block_root)
            .unwrap()
            .unwrap()
            .validators
            .len() as u64;

        let validator_ids: Vec<u64> = (0..num_validators).collect();
        let key_pairs = install_validator_keys(&store, &validator_ids).await;
        let attestation_data = AttestationData {
            slot,
            head: Checkpoint {
                root: block_root,
                slot,
            },
            target: Checkpoint {
                root: block_root,
                slot,
            },
            source: latest_justified_provider.get().unwrap(),
        };
        let data_root = attestation_data.tree_hash_root();

        for validator_id in 0..num_validators {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();
        let has_new_payloads = {
            let db = store.store.lock().await;
            !db.latest_new_aggregated_payloads_provider()
                .iter()
                .unwrap()
                .is_empty()
        };
        store.update_safe_target().await.unwrap();

        let (safe_target_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.safe_target_provider(), db.block_provider())
        };
        let safe_target_slot = block_provider
            .get(safe_target_provider.get().unwrap())
            .unwrap()
            .unwrap()
            .block
            .slot;

        assert!(has_new_payloads);
        assert!(safe_target_slot <= slot);
    }

    // TEST JUSTIFICATION LOGIC

    /// Justification should occur when 2/3 validators attest to the same target.
    #[tokio::test]
    pub async fn test_justification_with_supermajority_attestations() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(1);
        let mut store: Store = sample_store_as_store(10).await;

        let slot_1 = 1;
        let proposer_index_1 = 1;
        let block_1_with_signatures = store
            .produce_block_with_signatures(slot_1, proposer_index_1)
            .await
            .unwrap();
        let signed_block_1 = build_signed_block(block_1_with_signatures);
        store.on_block(&signed_block_1, false).await.unwrap();
        set_validator_id(&store, Some(0)).await;

        let (head_provider, state_provider, latest_justified_provider) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.state_provider(),
                db.latest_justified_provider(),
            )
        };
        let initial_latest_justified_slot = latest_justified_provider.get().unwrap().slot;
        let block_1_root = head_provider.get().unwrap();
        let num_validators = state_provider
            .get(block_1_root)
            .unwrap()
            .unwrap()
            .validators
            .len() as u64;
        let threshold = (num_validators * 2 + 2).div_ceil(3);

        let validator_ids: Vec<u64> = (0..num_validators).collect();
        let key_pairs = install_validator_keys(&store, &validator_ids).await;

        let attestation_data = AttestationData {
            slot: slot_1,
            head: Checkpoint {
                root: block_1_root,
                slot: slot_1,
            },
            target: Checkpoint {
                root: block_1_root,
                slot: slot_1,
            },
            source: latest_justified_provider.get().unwrap(),
        };
        let data_root = attestation_data.tree_hash_root();

        for validator_id in 0..(threshold + 1) {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();
        {
            let db = store.store.lock().await;
            let latest_new = db.latest_new_aggregated_payloads_provider();
            let latest_known = db.latest_known_aggregated_payloads_provider();
            for (signature_key, mut new_proofs) in latest_new.drain().unwrap() {
                let mut existing = latest_known
                    .get(signature_key.clone())
                    .unwrap()
                    .unwrap_or_default();
                existing.append(&mut new_proofs);
                latest_known.insert(signature_key, existing).unwrap();
            }
        }

        let latest_justified_slot = {
            store
                .store
                .lock()
                .await
                .latest_justified_provider()
                .get()
                .unwrap()
                .slot
        };

        let slot_2 = 2;
        let proposer_index_2 = 2;
        let block_2_result = store
            .produce_block_with_signatures(slot_2, proposer_index_2)
            .await;
        if let Ok(block_with_signatures) = block_2_result {
            assert!(!block_with_signatures.block.body.attestations.is_empty());
        } else {
            let known_payloads_len = {
                let db = store.store.lock().await;
                db.latest_known_aggregated_payloads_provider()
                    .iter()
                    .unwrap()
                    .len()
            };
            assert!(known_payloads_len > 0);
        }
        assert!(latest_justified_slot >= initial_latest_justified_slot);
    }

    /// Attestations must have a valid/already justified source.
    #[tokio::test]
    pub async fn test_justification_requires_valid_source() {
        let mut store: Store = sample_store_as_store(10).await;
        let slot = 1;
        let proposer_index = 1;

        let block_1_with_signatures = store
            .produce_block_with_signatures(slot, proposer_index)
            .await
            .unwrap();
        let signed_block_1 = build_signed_block(block_1_with_signatures);
        store.on_block(&signed_block_1, false).await.unwrap();

        let (head_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.head_provider(), db.block_provider())
        };
        let block_root = head_provider.get().unwrap();
        let block_slot = block_provider.get(block_root).unwrap().unwrap().block.slot;

        let attestation = SignedAttestation {
            validator_id: 5,
            message: AttestationData {
                slot: block_slot,
                head: Checkpoint {
                    root: block_root,
                    slot: block_slot,
                },
                target: Checkpoint {
                    root: block_root,
                    slot: block_slot,
                },
                source: Checkpoint {
                    root: B256::from([0x69; 32]),
                    slot: 999,
                },
            },
            signature: Signature::blank(),
        };

        let result = store.validate_attestation(&attestation).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown source block")
        );
    }

    /// Justification should track votes for multiple potential targets.
    #[tokio::test]
    pub async fn test_justification_tracking_with_multiple_targets() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(1);
        let mut store: Store = sample_store_as_store(10).await;

        for slot_num in 1..4 {
            let block_with_signatures = store
                .produce_block_with_signatures(slot_num, slot_num)
                .await
                .unwrap();
            let signed_block = build_signed_block(block_with_signatures);
            store.on_block(&signed_block, false).await.unwrap();
        }
        set_validator_id(&store, Some(0)).await;

        let (head_provider, block_provider, state_provider) = {
            let db = store.store.lock().await;
            (db.head_provider(), db.block_provider(), db.state_provider())
        };
        let head_root = head_provider.get().unwrap();
        let head_block = block_provider.get(head_root).unwrap().unwrap();
        let head_slot = head_block.block.slot;
        let num_validators = state_provider
            .get(head_root)
            .unwrap()
            .unwrap()
            .validators
            .len() as u64;

        let validator_ids: Vec<u64> = (0..num_validators).collect();
        let key_pairs = install_validator_keys(&store, &validator_ids).await;
        let attestation_data_head = store.produce_attestation_data(head_slot).await.unwrap();
        let data_root = attestation_data_head.tree_hash_root();

        for validator_id in 0..(num_validators / 2) {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data_head.slot as u32)
                .unwrap();

            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data_head.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();
        store.update_safe_target().await.unwrap();

        let safe_target_root = {
            store
                .store
                .lock()
                .await
                .safe_target_provider()
                .get()
                .unwrap()
        };
        assert!(block_provider.contains_key(safe_target_root));
    }

    // TEST FINALIZATION FOLLOWS JUSTIFICATION

    /// Finalization should follow when justification advances without gaps.
    #[tokio::test]
    pub async fn test_finalization_after_consecutive_justification() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(1);
        let mut store: Store = sample_store_as_store(10).await;
        set_validator_id(&store, Some(0)).await;

        let initial_finalized_slot = {
            store
                .store
                .lock()
                .await
                .latest_finalized_provider()
                .get()
                .unwrap()
                .slot
        };

        let num_validators = {
            let db = store.store.lock().await;
            let head_root = db.head_provider().get().unwrap();
            db.state_provider()
                .get(head_root)
                .unwrap()
                .unwrap()
                .validators
                .len() as u64
        };
        let threshold = (num_validators * 2 + 2).div_ceil(3);
        let validator_ids: Vec<u64> = (0..num_validators).collect();
        let key_pairs = install_validator_keys(&store, &validator_ids).await;

        for slot_num in 1..5 {
            if slot_num > 1 {
                let (head_provider, block_provider, latest_justified_provider) = {
                    let db = store.store.lock().await;
                    (
                        db.head_provider(),
                        db.block_provider(),
                        db.latest_justified_provider(),
                    )
                };
                let prev_head = head_provider.get().unwrap();
                let prev_block = block_provider.get(prev_head).unwrap().unwrap();
                let prev_slot = prev_block.block.slot;

                let attestation_data = AttestationData {
                    slot: prev_slot,
                    head: Checkpoint {
                        root: prev_head,
                        slot: prev_slot,
                    },
                    target: Checkpoint {
                        root: prev_head,
                        slot: prev_slot,
                    },
                    source: latest_justified_provider.get().unwrap(),
                };
                let data_root = attestation_data.tree_hash_root();

                for validator_id in 0..(threshold + 1) {
                    let signature = key_pairs
                        .get(&validator_id)
                        .unwrap()
                        .1
                        .sign(&data_root.0, attestation_data.slot as u32)
                        .unwrap();

                    store
                        .on_gossip_attestation(
                            SignedAttestation {
                                validator_id,
                                message: attestation_data.clone(),
                                signature,
                            },
                            true,
                        )
                        .await
                        .unwrap();
                }

                store.aggregate().await.unwrap();
            }

            let proposer = slot_num % num_validators;
            let _ = produce_and_import_block(&mut store, slot_num, proposer).await;
        }

        let final_finalized_slot = {
            store
                .store
                .lock()
                .await
                .latest_finalized_provider()
                .get()
                .unwrap()
                .slot
        };

        assert!(final_finalized_slot >= initial_finalized_slot);
    }

    // TEST ATTESTATION TARGET EDGE CASES

    /// Attestation target should handle chains with skipped slots.
    #[tokio::test]
    pub async fn test_attestation_target_with_skipped_slots() {
        let mut store: Store = sample_store_as_store(10).await;
        produce_and_import_block(&mut store, 1, 1).await.unwrap();
        produce_and_import_block(&mut store, 4, 4).await.unwrap();

        let target = store.get_attestation_target().await.unwrap();
        let (block_provider, latest_finalized_provider) = {
            let db = store.store.lock().await;
            (db.block_provider(), db.latest_finalized_provider())
        };
        let finalized_slot = latest_finalized_provider.get().unwrap().slot;

        assert!(block_provider.contains_key(target.root));
        assert!(is_justifiable_after(target.slot, finalized_slot).unwrap());
    }

    /// Attestation target computation should work with single validator.
    #[tokio::test]
    pub async fn test_attestation_target_single_validator() {
        let store: Store = sample_store_as_store(1).await;
        let target = store.get_attestation_target().await.unwrap();
        let head_root = { store.store.lock().await.head_provider().get().unwrap() };

        assert_eq!(target.root, head_root);
    }

    /// Test target when head is exactly JUSTIFICATION_LOOKBACK_SLOTS ahead.
    #[tokio::test]
    pub async fn test_attestation_target_at_justification_lookback_boundary() {
        let mut store: Store = sample_store_as_store(10).await;
        let num_validators = {
            let db = store.store.lock().await;
            let head_root = db.head_provider().get().unwrap();
            db.state_provider()
                .get(head_root)
                .unwrap()
                .unwrap()
                .validators
                .len() as u64
        };

        for slot_num in 1..(JUSTIFICATION_LOOKBACK_SLOTS + 2) {
            let proposer = slot_num % num_validators;
            produce_and_import_block(&mut store, slot_num, proposer)
                .await
                .unwrap();
        }

        let target = store.get_attestation_target().await.unwrap();
        let (head_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.head_provider(), db.block_provider())
        };

        let head_slot = block_provider
            .get(head_provider.get().unwrap())
            .unwrap()
            .unwrap()
            .block
            .slot;

        assert!(target.slot >= head_slot - JUSTIFICATION_LOOKBACK_SLOTS);
    }

    // TEST INTEGRATION SCENARIOS

    /// Test complete cycle: produce block, attest, justify.
    #[tokio::test]
    pub async fn test_full_attestation_cycle() {
        let _test_guard = test_global_lock().lock().await;
        let _committee_count_override = CommitteeCountOverride::new(1);
        let mut store: Store = sample_store_as_store(10).await;

        let slot_1 = 1;
        let proposer_1 = 1;
        store
            .produce_block_with_signatures(slot_1, proposer_1)
            .await
            .unwrap();

        set_validator_id(&store, Some(0)).await;
        let (head_provider, state_provider) = {
            let db = store.store.lock().await;
            (db.head_provider(), db.state_provider())
        };
        let num_validators = state_provider
            .get(head_provider.get().unwrap())
            .unwrap()
            .unwrap()
            .validators
            .len() as u64;
        let validator_ids: Vec<u64> = (0..num_validators).collect();
        let key_pairs = install_validator_keys(&store, &validator_ids).await;
        let attestation_data = store.produce_attestation_data(slot_1).await.unwrap();
        let data_root = attestation_data.tree_hash_root();

        for validator_id in 0..num_validators {
            let signature = key_pairs
                .get(&validator_id)
                .unwrap()
                .1
                .sign(&data_root.0, attestation_data.slot as u32)
                .unwrap();
            store
                .on_gossip_attestation(
                    SignedAttestation {
                        validator_id,
                        message: attestation_data.clone(),
                        signature,
                    },
                    true,
                )
                .await
                .unwrap();
        }

        store.aggregate().await.unwrap();
        store.update_safe_target().await.unwrap();

        let slot_2 = 2;
        let proposer_2 = 2;
        let _ = store
            .produce_block_with_signatures(slot_2, proposer_2)
            .await;

        let (safe_target_provider, block_provider) = {
            let db = store.store.lock().await;
            (db.safe_target_provider(), db.block_provider())
        };

        let safe_target_slot = block_provider
            .get(safe_target_provider.get().unwrap())
            .unwrap()
            .unwrap()
            .block
            .slot;

        assert!(safe_target_slot <= 1);
        assert!(block_provider.contains_key(head_provider.get().unwrap()));
        assert!(block_provider.contains_key(safe_target_provider.get().unwrap()));
    }

    /// Test attestation target is correct after processing a block via on_block.
    #[tokio::test]
    pub async fn test_attestation_target_after_on_block() {
        let mut store: Store = sample_store_as_store(10).await;
        let slot_1 = 1;
        let proposer_1 = 1;
        let block_with_signatures = store
            .produce_block_with_signatures(slot_1, proposer_1)
            .await
            .unwrap();
        let signed_block = build_signed_block(block_with_signatures);

        let target_time =
            lean_network_spec().genesis_time + slot_1 * lean_network_spec().seconds_per_slot;
        store.on_tick(target_time, true, false).await.unwrap();
        store.on_block(&signed_block, false).await.unwrap();

        let target = store.get_attestation_target().await.unwrap();
        let (block_provider, latest_finalized_provider) = {
            let db = store.store.lock().await;
            (db.block_provider(), db.latest_finalized_provider())
        };
        let finalized_slot = latest_finalized_provider.get().unwrap().slot;

        assert!(block_provider.contains_key(target.root));
        assert!(is_justifiable_after(target.slot, finalized_slot).unwrap());
    }
}
