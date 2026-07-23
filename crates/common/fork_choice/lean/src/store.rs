use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
#[cfg(feature = "devnet5")]
use ream_consensus_lean::attestation::SingleMessageAggregate as PayloadProof;
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation, AggregatedAttestations, AttestationData, SignatureKey,
        SignedAggregatedAttestation, SignedAttestation,
    },
    block::{Block, BlockBody, BlockWithSignatures, SignedBlock},
    checkpoint::Checkpoint,
    slot::{is_justifiable_after, justified_index_after},
    state::{LeanState, attestation_data_matches_chain},
    validator::is_proposer,
};
use ream_consensus_misc::constants::lean::{
    GOSSIP_DISPARITY_INTERVALS, INTERVALS_PER_SLOT, MAX_ATTESTATIONS_DATA,
    MAX_HISTORICAL_BLOCK_HASHES, attestation_committee_count,
};
use ream_metrics::{
    ATTESTATION_COMMITTEE_SUBNET, ATTESTATION_VALIDATION_TIME, ATTESTATIONS_INVALID_TOTAL,
    ATTESTATIONS_VALID_TOTAL, BLOCK_AGGREGATED_PAYLOADS, BLOCK_BUILDING_PAYLOAD_AGGREGATION_TIME,
    BLOCK_BUILDING_SUCCESS_TOTAL, BLOCK_BUILDING_TIME, COMMITTEE_SIGNATURES_AGGREGATION_TIME,
    FINALIZED_SLOT, FORK_CHOICE_BLOCK_PROCESSING_TIME, GOSSIP_SIGNATURES, HEAD_SLOT,
    JUSTIFIED_SLOT, LATEST_FINALIZED_SLOT, LATEST_JUSTIFIED_SLOT, LATEST_KNOWN_AGGREGATED_PAYLOADS,
    LATEST_NEW_AGGREGATED_PAYLOADS, LEAN_TICK_INTERVAL_DURATION_SECONDS,
    PQ_SIG_AGGREGATED_SIGNATURES_BUILDING_TIME, PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL,
    PQ_SIG_AGGREGATED_SIGNATURES_TOTAL, PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL,
    PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME, PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL,
    PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL, PQ_SIG_ATTESTATION_VERIFICATION_TIME,
    PQ_SIG_ATTESTATIONS_IN_AGGREGATED_SIGNATURES_TOTAL, PROPOSE_BLOCK_TIME, SAFE_TARGET_SLOT,
    inc_block_proposal_attestation_builds, inc_block_proposal_child_payloads_consumed,
    inc_int_counter_vec, inc_int_counter_vec_by, observe_block_proposal_aggregates_selected,
    observe_block_proposal_attestation_data_selected, observe_block_proposal_phase,
    observe_histogram_vec, set_int_gauge_vec, start_timer, stop_timer,
};
use ream_network_spec::networks::lean_network_spec;
use ream_network_state_lean::NetworkState;
#[cfg(feature = "devnet5")]
use ream_post_quantum_crypto::lean_multisig::type_2::{
    type_1_aggregate, type_1_from_wire, type_1_to_wire, type_1_verify,
};
#[cfg(feature = "devnet5")]
use ream_post_quantum_crypto::leansig::public_key::PublicKey;
use ream_post_quantum_crypto::leansig::signature::Signature;
use ream_storage::{
    db::lean::LeanDB,
    tables::{
        field::REDBField,
        lean::{
            block::LeanBlockTable, gossip_signatures::GossipSignaturesTable,
            latest_known_aggregated_payloads::LeanLatestKnownAggregatedPayloadsTable,
        },
        table::REDBTable,
    },
};
use ream_sync::rwlock::{Reader, Writer};
use ssz_types::{
    BitList, VariableList,
    typenum::{U4096, U262144},
};
use tokio::sync::Mutex;
use tree_hash::TreeHash;

use crate::constants::{ATTESTATION_RETENTION_SLOTS, JUSTIFICATION_LOOKBACK_SLOTS};

pub type LeanStoreWriter = Writer<Store>;
pub type LeanStoreReader = Reader<Store>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AttestationScoreTier {
    Finalize = 1,
    Justify = 2,
    Build = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AttestationEntryScore {
    tier: AttestationScoreTier,
    new_voter_count: usize,
    target_slot: u64,
    attestation_slot: u64,
}

impl AttestationEntryScore {
    fn ordering_key(&self, data_root: B256) -> AttestationEntryOrderingKey {
        AttestationEntryOrderingKey {
            tier: self.tier,
            new_voter_count: Reverse(self.new_voter_count),
            target_slot: self.target_slot,
            attestation_slot: self.attestation_slot,
            data_root,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct AttestationEntryOrderingKey {
    tier: AttestationScoreTier,
    new_voter_count: Reverse<usize>,
    target_slot: u64,
    attestation_slot: u64,
    data_root: B256,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlockProductionStrategy {
    #[default]
    RoundBased,
    Tiered,
}

struct BuildContext {
    head_state: LeanState,
    available_signed_attestations: HashMap<u64, SignedAttestation>,
    block_provider: LeanBlockTable,
    latest_known_aggregated_payloads_provider: LeanLatestKnownAggregatedPayloadsTable,
}

#[cfg(feature = "devnet5")]
pub struct AggregationJob {
    data: AttestationData,
    data_root: B256,
    child_wires: Vec<(Vec<u8>, Vec<PublicKey>)>,
    raw_xmss: Vec<(PublicKey, Signature)>,
    bits: BitList<U4096>,
    raw_count: u64,
}

#[cfg(feature = "devnet5")]
pub fn prove_aggregation_jobs(
    jobs: Vec<AggregationJob>,
) -> anyhow::Result<Vec<SignedAggregatedAttestation>> {
    let mut results = Vec::with_capacity(jobs.len());
    for job in jobs {
        let building_timer = start_timer(&PQ_SIG_AGGREGATED_SIGNATURES_BUILDING_TIME, &[]);
        let children = job
            .child_wires
            .iter()
            .map(|(wire, public_keys)| type_1_from_wire(wire, public_keys))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let type_one = type_1_aggregate(
            &children,
            &job.raw_xmss,
            &job.data_root.0,
            job.data.slot as u32,
        )?;
        let proof = PayloadProof {
            participants: job.bits.clone(),
            proof: VariableList::new(type_1_to_wire(&type_one))
                .map_err(|err| anyhow!("Failed to create proof_data: {err:?}"))?,
        };
        stop_timer(building_timer);
        inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_TOTAL, &[]);
        inc_int_counter_vec_by(
            &PQ_SIG_ATTESTATIONS_IN_AGGREGATED_SIGNATURES_TOTAL,
            job.raw_count,
            &[],
        );
        results.push(SignedAggregatedAttestation {
            data: job.data.clone(),
            proof,
        });
    }
    Ok(results)
}

/// [Store] represents the state that the Lean node should maintain.
#[derive(Debug, Clone)]
pub struct Store {
    pub store: Arc<Mutex<LeanDB>>,
    pub network_state: Arc<NetworkState>,
    pub tick_interval_duration: Option<Instant>,
    pub block_production_strategy: BlockProductionStrategy,
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

        let anchor_checkpoint = Checkpoint {
            root: anchor_root,
            slot: anchor_slot,
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
            .insert(anchor_checkpoint)
            .expect("Failed to insert latest finalized checkpoint");
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
        db.validator_id_provider()
            .insert(validator_id)
            .expect("Failed to insert validator id");

        Ok(Store {
            store: Arc::new(Mutex::new(db)),
            network_state: Arc::new(NetworkState::new(anchor_checkpoint, anchor_checkpoint)),
            tick_interval_duration: None,
            block_production_strategy: BlockProductionStrategy::default(),
        })
    }

    /// Override the block-production strategy. Defaults to round-based.
    pub fn with_block_production_strategy(mut self, strategy: BlockProductionStrategy) -> Self {
        self.block_production_strategy = strategy;
        self
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

        let (slot_index_table, block_provider, children_index_provider) = {
            let db = self.store.lock().await;
            (
                db.slot_index_provider(),
                db.block_provider(),
                db.children_index_provider(),
            )
        };

        let index_map = children_index_provider.get_index_map()?;

        // Start at genesis by default
        if root == B256::ZERO || !index_map.contains_key(&root) {
            root = slot_index_table
                .get_oldest_root()?
                .ok_or(anyhow!("No blocks found to calculate fork choice"))?;
        }
        let start_slot = match index_map.get(&root) {
            Some(&(_, slot)) => slot,
            None => {
                block_provider
                    .get(root)?
                    .ok_or(anyhow!("Failed to get block for root {root:?}"))?
                    .block
                    .slot
            }
        };
        // For each block, count the number of votes for that block. A vote
        // for any descendant of a block also counts as a vote for that block
        let mut weights = HashMap::<B256, u64>::new();

        for attestation in attestations {
            let attestation = attestation?;
            let mut current_root = attestation.message.head.root;

            while let Some(&(parent_root, slot)) = index_map.get(&current_root) {
                if slot <= start_slot {
                    break;
                }

                *weights.entry(current_root).or_insert(0) += 1;

                current_root = parent_root;
            }
        }

        // Identify the children of each block
        let mut children_map = HashMap::<B256, Vec<B256>>::new();
        for (&block_root, &(parent_root, _)) in &index_map {
            if parent_root == B256::ZERO {
                continue;
            }
            if min_score > 0 && *weights.get(&block_root).unwrap_or(&0) < min_score {
                continue;
            }
            children_map
                .entry(parent_root)
                .or_default()
                .push(block_root);
        }

        // Start at the root (latest justified hash or genesis) and repeatedly
        // choose the child with the most latest votes, tiebreaking by slot then hash
        let mut head = root;
        let mut head_slot = start_slot;

        while let Some(children) = children_map.get(&head) {
            (head, head_slot) = children
                .iter()
                .map(|child_hash| {
                    let vote_weight = *weights.get(child_hash).unwrap_or(&0);
                    let slot = index_map
                        .get(child_hash)
                        .map(|&(_, slot)| slot)
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

    async fn genesis_time(&self) -> anyhow::Result<u64> {
        let db = self.store.lock().await;
        let head_root = db.head_provider().get()?;
        let head_state = db
            .state_provider()
            .get(head_root)?
            .ok_or_else(|| anyhow!("Head state not found while reading genesis time"))?;

        Ok(head_state.config.genesis_time)
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
            let new_payload_keys = latest_new_aggregated_payloads_provider
                .iter()?
                .into_iter()
                .map(|(signature_key, _proofs)| signature_key)
                .collect::<Vec<_>>();

            self.extract_attestations_from_aggregated_payloads(&new_payload_keys)
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
        let (
            latest_new_aggregated_payloads_provider,
            latest_known_aggregated_payloads_provider,
            attestation_data_by_root_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.latest_new_aggregated_payloads_provider(),
                db.latest_known_aggregated_payloads_provider(),
                db.attestation_data_by_root_provider(),
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

        let known = latest_known_aggregated_payloads_provider.iter()?;
        let mut validator_max_slot: HashMap<u64, u64> = HashMap::new();
        let mut key_slots: Vec<(SignatureKey, u64)> = Vec::with_capacity(known.len());
        for (key, _) in &known {
            let slot = attestation_data_by_root_provider
                .get(key.data_root)?
                .map(|data| data.slot)
                .unwrap_or(0);
            key_slots.push((key.clone(), slot));
            validator_max_slot
                .entry(key.validator_id)
                .and_modify(|max| *max = (*max).max(slot))
                .or_insert(slot);
        }
        let superseded: HashSet<SignatureKey> = key_slots
            .into_iter()
            .filter(|(key, slot)| *slot < validator_max_slot[&key.validator_id])
            .map(|(key, _)| key)
            .collect();
        if !superseded.is_empty() {
            latest_known_aggregated_payloads_provider.retain(|key, _| !superseded.contains(key))?;
        }

        set_int_gauge_vec(
            &LATEST_KNOWN_AGGREGATED_PAYLOADS,
            latest_known_aggregated_payloads_provider.entry_count()? as i64,
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
            #[cfg(feature = "devnet5")]
            let _ = is_aggregator;
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
        if let Some(instant) = self.tick_interval_duration {
            LEAN_TICK_INTERVAL_DURATION_SECONDS
                .with_label_values(&[])
                .observe(instant.elapsed().as_secs_f64());
        }
        self.tick_interval_duration = Some(Instant::now());

        let genesis_time = self.genesis_time().await?;
        let Some(seconds_since_genesis) = time.checked_sub(genesis_time) else {
            return Ok(());
        };
        let time_delta_ms = seconds_since_genesis * 1000;
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
        let (
            latest_justified_provider,
            latest_finalized_provider,
            head_provider,
            block_provider,
            state_provider,
            latest_known_aggregated_payloads_provider,
            attestation_data_by_root_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.latest_justified_provider(),
                db.latest_finalized_provider(),
                db.head_provider(),
                db.block_provider(),
                db.state_provider(),
                db.latest_known_aggregated_payloads_provider(),
                db.attestation_data_by_root_provider(),
            )
        };

        let latest_finalized_checkpoint = latest_finalized_provider.get()?;
        let finalized_slot = latest_finalized_checkpoint.slot;
        let attestations = {
            let mut relevant_keys = Vec::new();
            for key in latest_known_aggregated_payloads_provider.iter_keys()? {
                if let Some(data) = attestation_data_by_root_provider.get(key.data_root)?
                    && data.head.slot > finalized_slot
                {
                    relevant_keys.push(key);
                }
            }

            self.extract_attestations_from_aggregated_payloads(&relevant_keys)
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

        let target_finalized_slot = state_provider
            .get(new_head)?
            .ok_or(anyhow!("State not found"))?
            .latest_finalized
            .slot;
        let mut finalized_root = new_head;

        let index_map = {
            let db = self.store.lock().await;
            db.children_index_provider().get_index_map()?
        };
        let lookup_slot = |root: B256| -> anyhow::Result<Option<(B256, u64)>> {
            if let Some(&(parent_root, slot)) = index_map.get(&root) {
                return Ok(Some((parent_root, slot)));
            }
            Ok(block_provider
                .get(root)?
                .map(|block| (block.block.parent_root, block.block.slot)))
        };

        while let Some((parent_root, slot)) = lookup_slot(finalized_root)? {
            if slot <= target_finalized_slot {
                break;
            }
            finalized_root = parent_root;
        }

        let final_finalized_checkpoint =
            if lookup_slot(finalized_root)?.map(|(_, slot)| slot) == Some(target_finalized_slot) {
                Checkpoint {
                    root: finalized_root,
                    slot: target_finalized_slot,
                }
            } else {
                latest_finalized_checkpoint
            };

        set_int_gauge_vec(&HEAD_SLOT, new_head_slot as i64, &[]);
        set_int_gauge_vec(&FINALIZED_SLOT, final_finalized_checkpoint.slot as i64, &[]);
        set_int_gauge_vec(
            &LATEST_FINALIZED_SLOT,
            final_finalized_checkpoint.slot as i64,
            &[],
        );
        *self.network_state.head_checkpoint.write() = Checkpoint {
            root: new_head,
            slot: new_head_slot,
        };
        *self.network_state.finalized_checkpoint.write() = final_finalized_checkpoint;

        head_provider.insert(new_head)?;
        latest_finalized_provider.insert(final_finalized_checkpoint)?;

        Ok(())
    }

    pub async fn get_attestation_target(&self) -> anyhow::Result<Checkpoint> {
        let (head_provider, block_provider, safe_target_provider, state_provider) = {
            let db = self.store.lock().await;
            (
                db.head_provider(),
                db.block_provider(),
                db.safe_target_provider(),
                db.state_provider(),
            )
        };

        let head_root = head_provider.get()?;

        let head_state = state_provider
            .get(head_root)?
            .ok_or(anyhow!("Head state not found for attestation target"))?;
        let head_finalized_slot = head_state.latest_finalized.slot;
        let head_justified = head_state.latest_justified;

        let mut target_block_root = head_root;

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

        while !is_justifiable_after(
            block_provider
                .get(target_block_root)?
                .ok_or(anyhow!("Block not found for target block root"))?
                .block
                .slot,
            head_finalized_slot,
        ) {
            target_block_root = block_provider
                .get(target_block_root)?
                .ok_or(anyhow!("Block not found for target block root"))?
                .block
                .parent_root;
        }

        let target_block = block_provider
            .get(target_block_root)?
            .ok_or(anyhow!("Block not found for target block root"))?;

        if target_block.block.slot < head_justified.slot {
            return Ok(head_justified);
        }

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
        new_payloads: Option<&HashMap<AttestationData, HashSet<PayloadProof>>>,
        known_payloads: Option<&HashMap<AttestationData, HashSet<PayloadProof>>>,
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

                if child_proofs.is_empty() && raw_entries.len() <= 1 {
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

            let building_timer = start_timer(&PQ_SIG_AGGREGATED_SIGNATURES_BUILDING_TIME, &[]);

            #[cfg(feature = "devnet5")]
            let proof = {
                let children: Vec<_> = child_proofs
                    .iter()
                    .map(|child| {
                        let public_keys = child
                            .to_validator_indices()
                            .into_iter()
                            .map(|validator_id| {
                                head_state
                                    .validators
                                    .get(validator_id as usize)
                                    .map(|validator| validator.attestation_public_key)
                                    .ok_or_else(|| {
                                        anyhow!(
                                            "Validator index {validator_id} out of range during aggregation"
                                        )
                                    })
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        type_1_from_wire(&child.proof, &public_keys)
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let raw_xmss: Vec<_> = raw_entries
                    .iter()
                    .map(|(_, public_key, signature)| (*public_key, *signature))
                    .collect();
                let type_one =
                    type_1_aggregate(&children, &raw_xmss, &data_root.0, data.slot as u32)?;
                PayloadProof {
                    participants: bits.clone(),
                    proof: VariableList::new(type_1_to_wire(&type_one))
                        .map_err(|err| anyhow!("Failed to create proof_data: {err:?}"))?,
                }
            };

            stop_timer(building_timer);
            inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_TOTAL, &[]);
            inc_int_counter_vec_by(
                &PQ_SIG_ATTESTATIONS_IN_AGGREGATED_SIGNATURES_TOTAL,
                raw_entries.len() as u64,
                &[],
            );

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
    ) -> anyhow::Result<(Vec<AggregatedAttestation>, Vec<PayloadProof>)> {
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

        let (attestations, proofs): (Vec<_>, Vec<PayloadProof>) = results.into_iter().unzip();
        Ok((attestations, proofs))
    }

    async fn build_block(
        &self,
        strategy: BlockProductionStrategy,
        slot: u64,
        proposer_index: u64,
        parent_root: B256,
        attestations: Option<VariableList<AggregatedAttestations, U4096>>,
    ) -> anyhow::Result<(Block, Vec<PayloadProof>, LeanState)> {
        let ctx = self.load_build_context(parent_root).await?;
        let extended_historical_block_hashes =
            Self::extended_historical_block_hashes(&ctx.head_state, parent_root, slot);
        let (selected_attestations, child_payloads_consumed) = match strategy {
            BlockProductionStrategy::RoundBased => Self::select_round_based(
                &ctx,
                &extended_historical_block_hashes,
                attestations,
                slot,
                proposer_index,
                parent_root,
            )?,
            BlockProductionStrategy::Tiered => {
                let selected = Self::select_tiered(
                    &ctx,
                    &extended_historical_block_hashes,
                    attestations,
                    slot,
                )?;
                let child_payloads_consumed = selected.len() as u64;
                (selected, child_payloads_consumed)
            }
        };

        self.seal_block(
            &ctx.head_state,
            &selected_attestations,
            child_payloads_consumed,
            slot,
            proposer_index,
            parent_root,
        )
        .await
    }

    async fn load_build_context(&self, parent_root: B256) -> anyhow::Result<BuildContext> {
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

        Ok(BuildContext {
            head_state,
            available_signed_attestations,
            block_provider,
            latest_known_aggregated_payloads_provider,
        })
    }

    fn extended_historical_block_hashes(
        head_state: &LeanState,
        parent_root: B256,
        slot: u64,
    ) -> Vec<B256> {
        let num_empty_slots = slot
            .saturating_sub(head_state.latest_block_header.slot)
            .saturating_sub(1);
        let mut extended_historical_block_hashes = head_state.historical_block_hashes.to_vec();
        extended_historical_block_hashes.push(parent_root);
        extended_historical_block_hashes.extend(vec![B256::ZERO; num_empty_slots as usize]);

        extended_historical_block_hashes
    }

    fn select_round_based(
        ctx: &BuildContext,
        extended_historical_block_hashes: &[B256],
        attestations: Option<VariableList<AggregatedAttestations, U4096>>,
        slot: u64,
        proposer_index: u64,
        parent_root: B256,
    ) -> anyhow::Result<(Vec<AggregatedAttestations>, u64)> {
        let head_state = &ctx.head_state;
        let mut attestations: VariableList<AggregatedAttestations, U4096> =
            attestations.unwrap_or_else(VariableList::empty);

        let mut current_justified = if head_state.latest_block_header.slot == 0 {
            let mut justified_copy = head_state.latest_justified;
            justified_copy.root = parent_root;
            justified_copy
        } else {
            head_state.latest_justified
        };

        let mut current_finalized_slot = head_state.latest_finalized.slot;

        let mut current_justified_slots = head_state.justified_slots.clone();

        let mut processed_attestation_data: HashSet<AttestationData> = HashSet::new();

        let mut sorted_candidates: Vec<_> = ctx.available_signed_attestations.values().collect();
        sorted_candidates.sort_by_cached_key(|signed_attestation| {
            (
                signed_attestation.message.target.slot,
                signed_attestation.message.tree_hash_root(),
            )
        });

        let select_start = Instant::now();
        let mut child_payloads_consumed = 0;
        let mut total_stf_duration = Duration::default();
        loop {
            let mut new_attestations: VariableList<AggregatedAttestations, U4096> =
                VariableList::empty();

            for signed_attestation in &sorted_candidates {
                let data = &signed_attestation.message;

                if processed_attestation_data.len() >= MAX_ATTESTATIONS_DATA as usize
                    && !processed_attestation_data.contains(data)
                {
                    break;
                }

                if !ctx.block_provider.contains_key(data.head.root) {
                    continue;
                }

                if !(attestation_data_matches_chain(
                    extended_historical_block_hashes,
                    data.clone(),
                )?) {
                    continue;
                }

                let source_id = data.source.slot as usize;
                let current_source_justified = source_id < current_justified_slots.len()
                    && current_justified_slots.get(source_id).unwrap_or(false);

                let head_source_justified = source_id < head_state.justified_slots.len()
                    && head_state.justified_slots.get(source_id).unwrap_or(false);

                let source_is_justified = data.source.slot <= current_finalized_slot
                    || current_source_justified
                    || head_source_justified
                    || data.source == current_justified;

                if !source_is_justified {
                    continue;
                }

                let is_genesis_self_vote = data.source.slot == 0 && data.target.slot == 0;

                let target_id = data.target.slot as usize;
                let current_target_justified = target_id < current_justified_slots.len()
                    && current_justified_slots.get(target_id).unwrap_or(false);

                let head_target_justified = target_id < head_state.justified_slots.len()
                    && head_state.justified_slots.get(target_id).unwrap_or(false);

                let target_is_justified = data.target.slot <= current_finalized_slot
                    || current_target_justified
                    || head_target_justified
                    || data.target == current_justified;

                if !is_genesis_self_vote && target_is_justified {
                    continue;
                }

                if !is_genesis_self_vote
                    && !is_justifiable_after(data.target.slot, current_finalized_slot)
                {
                    continue;
                }

                let validator_id = signed_attestation.validator_id;
                let attestation = AggregatedAttestations {
                    validator_id,
                    data: data.clone(),
                };

                if attestations.contains(&attestation) {
                    continue;
                }

                let data_root = data.tree_hash_root();
                let signature_key = SignatureKey::from_parts(validator_id, data_root);
                let has_proof = ctx
                    .latest_known_aggregated_payloads_provider
                    .contains_key(&signature_key);

                if has_proof {
                    new_attestations
                        .push(attestation)
                        .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;

                    processed_attestation_data.insert(data.clone());
                    child_payloads_consumed += 1;
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

            let compact_start = Instant::now();
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

            observe_block_proposal_phase("compact", compact_start.elapsed());
            let stf_start = Instant::now();
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

            total_stf_duration += stf_start.elapsed();

            if advanced_state.latest_justified != current_justified
                || advanced_state.latest_finalized.slot != current_finalized_slot
            {
                current_justified = advanced_state.latest_justified;
                current_finalized_slot = advanced_state.latest_finalized.slot;
                current_justified_slots = advanced_state.justified_slots.clone();
                continue;
            }
            break;
        }
        observe_block_proposal_phase("stf_simulate", total_stf_duration);
        observe_block_proposal_phase("select_payloads", select_start.elapsed());

        Ok((attestations.to_vec(), child_payloads_consumed))
    }

    fn select_tiered(
        ctx: &BuildContext,
        extended_historical_block_hashes: &[B256],
        attestations: Option<VariableList<AggregatedAttestations, U4096>>,
        slot: u64,
    ) -> anyhow::Result<Vec<AggregatedAttestations>> {
        let head_state = &ctx.head_state;
        let mut candidates_by_data: HashMap<AttestationData, HashSet<u64>> = HashMap::new();
        for signed_attestation in ctx.available_signed_attestations.values() {
            candidates_by_data
                .entry(signed_attestation.message.clone())
                .or_default()
                .insert(signed_attestation.validator_id);
        }
        if let Some(attestations) = attestations {
            for attestation in attestations {
                candidates_by_data
                    .entry(attestation.data)
                    .or_default()
                    .insert(attestation.validator_id);
            }
        }

        // Ream stores payloads by validator/data root, so rebuild the per-data
        // proof pool that leanSpec receives directly.
        let mut aggregated_payloads: HashMap<B256, (AttestationData, Vec<PayloadProof>)> =
            HashMap::new();
        for (data, validator_ids) in candidates_by_data {
            if !ctx.block_provider.contains_key(data.head.root) {
                continue;
            }

            let data_root = data.tree_hash_root();
            let mut proofs = HashSet::new();
            for validator_id in validator_ids {
                if let Some(validator_proofs) = ctx
                    .latest_known_aggregated_payloads_provider
                    .get(SignatureKey::from_parts(validator_id, data_root))?
                {
                    proofs.extend(validator_proofs);
                }
            }

            if !proofs.is_empty() {
                aggregated_payloads.insert(data_root, (data, proofs.into_iter().collect()));
            }
        }

        let validator_count = head_state.validators.len();
        let mut finalized_slot = head_state.latest_finalized.slot;
        let mut justified_slots = head_state.justified_slots.clone();
        extend_projected_justified_slots(
            &mut justified_slots,
            finalized_slot,
            slot.saturating_sub(1),
        )?;

        let mut votes_by_target_root = build_running_votes_by_target_root(head_state)?;
        let mut processed_data_roots = HashSet::new();
        let mut selected_attestations = Vec::new();

        for _ in 0..MAX_ATTESTATIONS_DATA {
            let mut best_candidate: Option<(B256, AttestationEntryScore, HashSet<u64>)> = None;
            let mut best_candidate_key = None;

            for (data_root, (candidate_data, proofs)) in &aggregated_payloads {
                if processed_data_roots.contains(data_root) {
                    continue;
                }

                if !ctx.block_provider.contains_key(candidate_data.head.root) {
                    continue;
                }

                if !attestation_data_matches_chain(
                    extended_historical_block_hashes,
                    candidate_data.clone(),
                )? {
                    continue;
                }

                if !is_projected_slot_justified(
                    &justified_slots,
                    finalized_slot,
                    candidate_data.source.slot,
                ) {
                    continue;
                }

                let is_genesis_self_vote =
                    candidate_data.source.slot == 0 && candidate_data.target.slot == 0;

                if !is_genesis_self_vote {
                    if candidate_data.target.slot <= candidate_data.source.slot {
                        continue;
                    }

                    if !is_justifiable_after(candidate_data.target.slot, finalized_slot) {
                        continue;
                    }
                }

                let prior_voters = votes_by_target_root
                    .get(&candidate_data.target.root)
                    .cloned()
                    .unwrap_or_default();
                let mut new_voters = HashSet::new();
                for proof in proofs {
                    for validator_index in proof.to_validator_indices() {
                        if !prior_voters.contains(&validator_index) {
                            new_voters.insert(validator_index);
                        }
                    }
                }

                if new_voters.is_empty() {
                    continue;
                }

                let total_voters = prior_voters.len() + new_voters.len();
                let crosses_two_thirds = 3 * total_voters >= 2 * validator_count;
                // finalizes_source: source finalizes only if no slot strictly between
                // source and target is still justifiable (3SF-mini).
                let finalizes_source =
                    if crosses_two_thirds && candidate_data.source.slot > finalized_slot {
                        let mut no_intermediate_justifiable = true;
                        for intermediate_slot in
                            candidate_data.source.slot + 1..candidate_data.target.slot
                        {
                            if is_justifiable_after(intermediate_slot, finalized_slot) {
                                no_intermediate_justifiable = false;
                                break;
                            }
                        }
                        no_intermediate_justifiable
                    } else {
                        false
                    };

                let tier = if is_genesis_self_vote || !crosses_two_thirds {
                    AttestationScoreTier::Build
                } else if finalizes_source {
                    AttestationScoreTier::Finalize
                } else {
                    AttestationScoreTier::Justify
                };

                let score = AttestationEntryScore {
                    tier,
                    new_voter_count: new_voters.len(),
                    target_slot: candidate_data.target.slot,
                    attestation_slot: candidate_data.slot,
                };
                let candidate_key = score.ordering_key(*data_root);

                if best_candidate_key
                    .as_ref()
                    .is_none_or(|key| candidate_key < *key)
                {
                    best_candidate = Some((*data_root, score, new_voters));
                    best_candidate_key = Some(candidate_key);
                }
            }

            let Some((data_root, score, selected_new_voters)) = best_candidate else {
                break;
            };
            let (attestation_data, proofs) = aggregated_payloads
                .get(&data_root)
                .ok_or_else(|| anyhow!("Selected missing attestation data root {data_root:?}"))?;

            processed_data_roots.insert(data_root);
            let selected_participants = proofs
                .iter()
                .flat_map(|proof| proof.to_validator_indices())
                .collect::<HashSet<_>>();
            for validator_id in selected_participants {
                selected_attestations.push(AggregatedAttestations {
                    validator_id,
                    data: attestation_data.clone(),
                });
            }

            if score.tier <= AttestationScoreTier::Justify {
                set_projected_justified_slot(
                    &mut justified_slots,
                    finalized_slot,
                    attestation_data.target.slot,
                )?;
                votes_by_target_root.remove(&attestation_data.target.root);
            } else {
                votes_by_target_root
                    .entry(attestation_data.target.root)
                    .or_default()
                    .extend(selected_new_voters);
            }

            if score.tier == AttestationScoreTier::Finalize {
                shift_projected_finalized_slot(
                    &mut justified_slots,
                    finalized_slot,
                    attestation_data.source.slot,
                )?;
                finalized_slot = attestation_data.source.slot;
            }
        }

        // Emission covers the full proof-union for the data, like leanSpec's
        // select_proofs_for_coverage. Projection above still uses new voters only.
        Ok(selected_attestations)
    }

    async fn seal_block(
        &self,
        head_state: &LeanState,
        attestations: &[AggregatedAttestations],
        child_payloads_consumed: u64,
        slot: u64,
        proposer_index: u64,
        parent_root: B256,
    ) -> anyhow::Result<(Block, Vec<PayloadProof>, LeanState)> {
        let payload_aggregation_timer = start_timer(&BLOCK_BUILDING_PAYLOAD_AGGREGATION_TIME, &[]);
        let aggregator_start = Instant::now();
        let (aggregated_attestations, aggregated_proofs) =
            self.select_aggregated_proofs(attestations).await?;
        observe_block_proposal_phase("proof_aggregation", aggregator_start.elapsed());
        stop_timer(payload_aggregation_timer);
        observe_histogram_vec(
            &BLOCK_AGGREGATED_PAYLOADS,
            aggregated_proofs.len() as f64,
            &[],
        );

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

        inc_block_proposal_attestation_builds();
        inc_block_proposal_child_payloads_consumed(child_payloads_consumed);
        observe_block_proposal_attestation_data_selected(
            candidate_final_block.body.attestations.len(),
        );
        observe_block_proposal_aggregates_selected(aggregated_proofs.len());

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
        let (state_provider, latest_known_aggregated_payloads_provider) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.latest_known_aggregated_payloads_provider(),
            )
        };

        let head_root = self.get_proposal_head(slot).await?;
        let building_timer = start_timer(&BLOCK_BUILDING_TIME, &[]);
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
            let signature_keys = latest_known_aggregated_payloads_provider.iter_keys()?;

            self.extract_attestations_from_aggregated_payloads(&signature_keys)
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
            .build_block(
                self.block_production_strategy,
                slot,
                validator_index,
                head_root,
                Some(attestation_list),
            )
            .await?;

        stop_timer(add_attestations_timer);

        let compute_state_root_timer = start_timer(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        candidate_block.state_root = post_state.tree_hash_root();
        stop_timer(compute_state_root_timer);

        #[cfg(feature = "devnet5")]
        let attestation_public_keys: Vec<Vec<PublicKey>> = proofs
            .iter()
            .map(|proof| {
                proof
                    .to_validator_indices()
                    .into_iter()
                    .map(|validator_id| {
                        head_state
                            .validators
                            .get(validator_id as usize)
                            .map(|validator| validator.attestation_public_key)
                            .ok_or_else(|| {
                                anyhow!("Proof references validator {validator_id} out of range")
                            })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let signatures_list = VariableList::new(proofs)
            .map_err(|err| anyhow!("Failed to return signatures {err:?}"))?;

        stop_timer(building_timer);
        inc_int_counter_vec(&BLOCK_BUILDING_SUCCESS_TOTAL, &[]);

        Ok(BlockWithSignatures {
            block: candidate_block,
            signatures: signatures_list,
            #[cfg(feature = "devnet5")]
            attestation_public_keys,
        })
    }

    pub async fn on_block(
        &mut self,
        signed_block: &SignedBlock,
        verify_signatures: bool,
    ) -> anyhow::Result<()> {
        let block_processing_timer = start_timer(&FORK_CHOICE_BLOCK_PROCESSING_TIME, &[]);

        let db = self.store.lock().await;
        let state_provider = db.state_provider();
        let block_provider = db.block_provider();
        let latest_justified_provider = db.latest_justified_provider();
        let attestation_data_by_root_provider = db.attestation_data_by_root_provider();
        let time_provider = db.time_provider();
        let latest_known_aggregated_payloads_provider =
            db.latest_known_aggregated_payloads_provider();
        drop(db);

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

        // A block far in the future, would unboundly spin the empty-slot loop that
        // is present in the state_transition -> process_slots. These checks reject
        // such blocks before they are processed.
        ensure!(
            block.slot.saturating_sub(parent_state.slot) <= MAX_HISTORICAL_BLOCK_HASHES,
            "Block slot is too far beyond its parent"
        );
        let current_slot = time_provider.get()? / INTERVALS_PER_SLOT;
        ensure!(block.slot <= current_slot + 1, "Block too far in future");

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

        set_int_gauge_vec(&JUSTIFIED_SLOT, latest_justified.slot as i64, &[]);
        set_int_gauge_vec(&LATEST_JUSTIFIED_SLOT, latest_justified.slot as i64, &[]);
        block_provider.insert_ref(block_root, signed_block)?;
        state_provider.insert(block_root, parent_state)?;
        latest_justified_provider.insert(latest_justified)?;
        let aggregated_attestations = &block.body.attestations;

        let mut seen_attestation_data = HashSet::with_capacity(aggregated_attestations.len());
        for attestation in aggregated_attestations.iter() {
            let data_root = attestation.message.tree_hash_root();
            ensure!(
                seen_attestation_data.insert(data_root),
                "Block contains duplicate AttestationData entries; \
                 each AttestationData must appear at most once",
            );
        }

        #[cfg(feature = "devnet5")]
        {
            for attestation in aggregated_attestations.iter() {
                let data_root = attestation.message.tree_hash_root();
                attestation_data_by_root_provider.insert(data_root, attestation.message.clone())?;

                let payload =
                    PayloadProof::new(attestation.aggregation_bits.clone(), VariableList::empty());

                for (validator_id, participated) in attestation.aggregation_bits.iter().enumerate()
                {
                    if !participated {
                        continue;
                    }
                    let key = SignatureKey::from_parts(validator_id as u64, data_root);
                    let mut existing_proofs = latest_known_aggregated_payloads_provider
                        .get(key.clone())?
                        .unwrap_or_default();
                    existing_proofs.push(payload.clone());
                    latest_known_aggregated_payloads_provider.insert(key, existing_proofs)?;
                }
            }
        }

        self.update_head().await?;

        stop_timer(block_processing_timer);
        Ok(())
    }

    pub async fn checkpoint_is_ancestor(
        &self,
        ancestor: &Checkpoint,
        descendant: &Checkpoint,
    ) -> anyhow::Result<bool> {
        if ancestor.slot > descendant.slot {
            return Ok(false);
        }

        let db = self.store.lock().await;

        let mut current_root = descendant.root;

        while let Some(block_wrapper) = db.block_provider().get(current_root)? {
            let block = &block_wrapper.block;

            if block.slot == ancestor.slot {
                return Ok(current_root == ancestor.root);
            }

            if block.slot < ancestor.slot {
                break;
            }

            current_root = block.parent_root;
        }

        Ok(false)
    }

    pub async fn validate_attestation(
        &self,
        signed_attestation: &SignedAttestation,
    ) -> anyhow::Result<()> {
        let timer = start_timer(&ATTESTATION_VALIDATION_TIME, &[]);
        let data = &signed_attestation.message;

        let (block_provider, time_provider, latest_finalized_provider) = {
            let db = self.store.lock().await;
            (
                db.block_provider(),
                db.time_provider(),
                db.latest_finalized_provider(),
            )
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

        if block_provider.contains_key(data.source.root)
            && block_provider.contains_key(data.target.root)
        {
            ensure!(
                self.checkpoint_is_ancestor(&data.source, &data.target)
                    .await?,
                "Source checkpoint must be ancestor of target"
            );
        }
        if block_provider.contains_key(data.target.root)
            && block_provider.contains_key(data.head.root)
        {
            ensure!(
                self.checkpoint_is_ancestor(&data.target, &data.head)
                    .await?,
                "Target checkpoint must be ancestor of head"
            );
        }

        // Fork choice only ever descends from the finalized block.
        ensure!(
            self.checkpoint_is_ancestor(&latest_finalized_provider.get()?, &data.head)
                .await?,
            "Head checkpoint must descend from the finalized block"
        );

        ensure!(
            data.slot >= head_block.block.slot,
            "Attestation slot precedes head"
        );

        let current_time = time_provider.get()?;
        let attestation_start_interval = data.slot * INTERVALS_PER_SLOT;

        ensure!(
            attestation_start_interval <= current_time + GOSSIP_DISPARITY_INTERVALS,
            "Attestation too far in future"
        );

        stop_timer(timer);
        Ok(())
    }

    pub async fn on_gossip_aggregated_attestation(
        &mut self,
        signed_attestation: SignedAggregatedAttestation,
    ) -> anyhow::Result<()> {
        self.on_gossip_aggregated_attestation_core(signed_attestation, true)
            .await
    }

    /// Process a gossiped aggregated attestation WITHOUT verifying its
    /// cryptographic proof.
    ///
    /// Only for spec-test fixtures whose proofs are mocked placeholders
    /// (`proofSetting == 0`, carrying leanSpec's `MOCK_PROOF_PREFIX`);
    pub async fn on_gossip_aggregated_attestation_without_verification(
        &mut self,
        signed_attestation: SignedAggregatedAttestation,
    ) -> anyhow::Result<()> {
        self.on_gossip_aggregated_attestation_core(signed_attestation, false)
            .await
    }

    async fn on_gossip_aggregated_attestation_core(
        &mut self,
        signed_attestation: SignedAggregatedAttestation,
        verify: bool,
    ) -> anyhow::Result<()> {
        match self
            .validate_attestation(&SignedAttestation {
                validator_id: 0,
                message: signed_attestation.data.clone(),
                signature: Signature::blank(),
            })
            .await
        {
            Ok(()) => inc_int_counter_vec(&ATTESTATIONS_VALID_TOTAL, &[]),
            Err(err) => {
                inc_int_counter_vec(&ATTESTATIONS_INVALID_TOTAL, &[]);
                return Err(err);
            }
        }

        let (
            attestation_data_by_root_provider,
            latest_new_aggregated_payloads_provider,
            latest_known_aggregated_payloads_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.attestation_data_by_root_provider(),
                db.latest_new_aggregated_payloads_provider(),
                db.latest_known_aggregated_payloads_provider(),
            )
        };

        {
            let data = &signed_attestation.data;
            let proof = &signed_attestation.proof;

            let data_root = data.tree_hash_root();
            let validator_ids = proof.to_validator_indices();

            ensure!(
                !validator_ids.is_empty(),
                "Aggregated attestation has no participants"
            );

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

            // Mocked spec-test proofs (`verify == false`) cannot be checked
            if verify {
                let verification_timer =
                    start_timer(&PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME, &[]);

                #[cfg(feature = "devnet5")]
                let verification_result = type_1_from_wire(proof.proof.as_ref(), &public_keys)
                    .and_then(|type_one| type_1_verify(&type_one));

                match verification_result {
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
            }

            attestation_data_by_root_provider.insert(data_root, data.clone())?;

            for &validator in &validator_ids {
                let mut already_voted_this_slot = false;
                for (key, _) in latest_new_aggregated_payloads_provider
                    .iter()?
                    .into_iter()
                    .chain(latest_known_aggregated_payloads_provider.iter()?)
                {
                    if key.validator_id != validator || key.data_root == data_root {
                        continue;
                    }
                    if attestation_data_by_root_provider
                        .get(key.data_root)?
                        .is_some_and(|existing_data| existing_data.slot == data.slot)
                    {
                        already_voted_this_slot = true;
                        break;
                    }
                }

                if already_voted_this_slot {
                    continue;
                }

                let key = SignatureKey::from_parts(validator, data_root);

                let mut proofs = latest_new_aggregated_payloads_provider
                    .get(key.clone())?
                    .unwrap_or_default();

                proofs.push(proof.clone());

                latest_new_aggregated_payloads_provider.insert(key, proofs)?;
            }
        }

        Ok(())
    }

    pub async fn extract_attestations_from_aggregated_payloads(
        &self,
        signature_keys: &[SignatureKey],
    ) -> anyhow::Result<HashMap<u64, AttestationData>> {
        let attestation_data_by_root_provider =
            self.store.lock().await.attestation_data_by_root_provider();
        let mut resolved_attestations = Vec::with_capacity(signature_keys.len());

        for signature_key in signature_keys {
            let data_root = signature_key.data_root;
            let Some(attestation_data) = attestation_data_by_root_provider.get(data_root)? else {
                continue;
            };

            resolved_attestations.push((signature_key.validator_id, data_root, attestation_data));
        }

        resolved_attestations.sort_by_key(|(_validator_id, data_root, data)| {
            std::cmp::Reverse((data.slot, *data_root))
        });

        let mut attestations: HashMap<u64, AttestationData> = HashMap::new();
        for (validator_id, _data_root, attestation_data) in resolved_attestations {
            attestations.entry(validator_id).or_insert(attestation_data);
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

        let signature_keys = attestation_signatures_provider.get_keys()?;
        set_int_gauge_vec(&GOSSIP_SIGNATURES, signature_keys.len() as i64, &[]);

        let mut attestation_signatures = Vec::new();
        for signature_key in signature_keys {
            if let Some(attestation_data) =
                attestation_data_by_root_provider.get(signature_key.data_root)?
            {
                attestation_signatures.push(AggregatedAttestations {
                    validator_id: signature_key.validator_id,
                    data: attestation_data,
                });
            }
        }

        let mut new_payloads: HashMap<AttestationData, HashSet<PayloadProof>> = HashMap::new();
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

        let mut known_payloads: HashMap<AttestationData, HashSet<PayloadProof>> = HashMap::new();
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

        let aggregation_timer = start_timer(&COMMITTEE_SIGNATURES_AGGREGATION_TIME, &[]);
        let signed_attestations = self.state_aggregate(
            &head_state,
            &attestation_signatures,
            &attestation_signatures_provider,
            Some(&new_payloads),
            Some(&known_payloads),
            true,
        )?;
        stop_timer(aggregation_timer);

        let mut aggregated_data_roots = HashSet::new();
        let mut next_new_payloads: HashMap<SignatureKey, Vec<PayloadProof>> = HashMap::new();

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

    #[cfg(feature = "devnet5")]
    pub async fn aggregate_prepare(&self) -> anyhow::Result<Vec<AggregationJob>> {
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

        let signature_keys = attestation_signatures_provider.get_keys()?;
        set_int_gauge_vec(&GOSSIP_SIGNATURES, signature_keys.len() as i64, &[]);

        let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();
        for signature_key in &signature_keys {
            if let Some(attestation_data) =
                attestation_data_by_root_provider.get(signature_key.data_root)?
            {
                groups
                    .entry(attestation_data)
                    .or_default()
                    .push(signature_key.validator_id);
            }
        }

        let mut new_payloads: HashMap<AttestationData, HashSet<PayloadProof>> = HashMap::new();
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

        let mut known_payloads: HashMap<AttestationData, HashSet<PayloadProof>> = HashMap::new();
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

        let mut keys: HashSet<AttestationData> = groups.keys().cloned().collect();
        keys.extend(new_payloads.keys().cloned());

        const AGG_RECENT_SLOTS: u64 = 16;
        let head_slot = head_state.slot;
        keys.retain(|data| data.slot + AGG_RECENT_SLOTS >= head_slot);

        let mut jobs = Vec::new();
        for data in keys {
            let data_root = data.tree_hash_root();
            let mut child_proofs = Vec::new();
            let mut covered_validators = HashSet::new();

            head_state.extend_proofs_greedily(
                new_payloads.get(&data),
                &mut child_proofs,
                &mut covered_validators,
            );
            head_state.extend_proofs_greedily(
                known_payloads.get(&data),
                &mut child_proofs,
                &mut covered_validators,
            );

            let mut raw_entries = Vec::new();
            if let Some(validator_ids) = groups.get(&data) {
                let mut sorted_ids = validator_ids.clone();
                sorted_ids.sort();
                for &validator_id in &sorted_ids {
                    if covered_validators.contains(&validator_id) {
                        continue;
                    }
                    if let Ok(Some(signature)) = attestation_signatures_provider
                        .get(SignatureKey::from_parts(validator_id, data_root))
                        && let Some(validator) = head_state.validators.get(validator_id as usize)
                    {
                        raw_entries.push((
                            validator_id,
                            validator.attestation_public_key,
                            signature,
                        ));
                        covered_validators.insert(validator_id);
                    }
                }
            }

            if raw_entries.is_empty() && child_proofs.len() < 2 {
                continue;
            }
            raw_entries.sort_by_key(|entry| entry.0);

            let mut bits = BitList::<U4096>::with_capacity(head_state.validators.len())
                .map_err(|err| anyhow!("BitList error: {err:?}"))?;
            for id in &covered_validators {
                bits.set(*id as usize, true)
                    .map_err(|err| anyhow!("Failed to set bits: {err:?}"))?;
            }

            let mut child_wires = Vec::with_capacity(child_proofs.len());
            for child in &child_proofs {
                let public_keys = child
                    .to_validator_indices()
                    .into_iter()
                    .map(|validator_id| {
                        head_state
                            .validators
                            .get(validator_id as usize)
                            .map(|validator| validator.attestation_public_key)
                            .ok_or_else(|| {
                                anyhow!(
                                    "Validator index {validator_id} out of range during aggregation"
                                )
                            })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                child_wires.push((child.proof.to_vec(), public_keys));
            }

            let raw_xmss = raw_entries
                .iter()
                .map(|(_, public_key, signature)| (*public_key, *signature))
                .collect();
            let raw_count = raw_entries.len() as u64;

            jobs.push(AggregationJob {
                data,
                data_root,
                child_wires,
                raw_xmss,
                bits,
                raw_count,
            });
        }
        jobs.sort_by_key(|job| Reverse(job.data.slot));
        Ok(jobs)
    }

    #[cfg(feature = "devnet5")]
    pub async fn aggregate_apply(
        &self,
        signed_attestations: &[SignedAggregatedAttestation],
    ) -> anyhow::Result<()> {
        let (latest_new_aggregated_payloads_provider, attestation_signatures_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_new_aggregated_payloads_provider(),
                db.attestation_signatures_provider(),
            )
        };

        let mut aggregated_data_roots = HashSet::new();
        let mut next_new_payloads: HashMap<SignatureKey, Vec<PayloadProof>> = HashMap::new();
        for signed_attestation in signed_attestations {
            let data_root = signed_attestation.data.tree_hash_root();
            aggregated_data_roots.insert(data_root);
            for validator_id in signed_attestation.proof.to_validator_indices() {
                next_new_payloads
                    .entry(SignatureKey::from_parts(validator_id, data_root))
                    .or_default()
                    .push(signed_attestation.proof.clone());
            }
        }

        for (key, proofs) in next_new_payloads {
            let mut existing = latest_new_aggregated_payloads_provider
                .get(key.clone())?
                .unwrap_or_default();
            for proof in proofs {
                if !existing.contains(&proof) {
                    existing.push(proof);
                }
            }
            latest_new_aggregated_payloads_provider.insert(key, existing)?;
        }

        attestation_signatures_provider
            .retain(|key| !aggregated_data_roots.contains(&key.data_root))?;

        Ok(())
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

        let signature_keys = latest_known_aggregated_payloads_provider.iter_keys()?;

        let attestations = self
            .extract_attestations_from_aggregated_payloads(&signature_keys)
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

        match self.validate_attestation(&signed_attestation).await {
            Ok(()) => inc_int_counter_vec(&ATTESTATIONS_VALID_TOTAL, &[]),
            Err(err) => {
                inc_int_counter_vec(&ATTESTATIONS_INVALID_TOTAL, &[]);
                return Err(err);
            }
        }

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
                compute_subnet_id(current_id, attestation_committee_count());
            set_int_gauge_vec(
                &ATTESTATION_COMMITTEE_SUBNET,
                current_validator_subnet as i64,
                &[],
            );
            let attester_subnet = compute_subnet_id(validator_id, attestation_committee_count());

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

        let target = self.get_attestation_target().await?;
        ensure!(
            source.slot <= target.slot,
            "Source must be older or equal to the target"
        );
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
            target,
            source,
        })
    }

    /// Fork choice only ever descends from the latest finalized block. This is sound only
    /// because the finalized checkpoint is re-derived from the head each update; pruning
    /// against a finalized checkpoint that drifted off the head chain would be unsound.
    pub async fn prune_stale_attestation_data(&mut self) -> anyhow::Result<()> {
        let (
            latest_finalized_provider,
            attestation_signatures_provider,
            attestation_data_by_root_provider,
            latest_new_aggregated_payloads_provider,
            latest_known_aggregated_payloads_provider,
            children_index_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.latest_finalized_provider(),
                db.attestation_signatures_provider(),
                db.attestation_data_by_root_provider(),
                db.latest_new_aggregated_payloads_provider(),
                db.latest_known_aggregated_payloads_provider(),
                db.children_index_provider(),
            )
        };

        let finalized_slot = latest_finalized_provider.get()?.slot;

        children_index_provider.prune_finalized(finalized_slot)?;

        let head_slot = self.network_state.head_checkpoint.read().slot;
        let cutoff_slot = finalized_slot.max(head_slot.saturating_sub(ATTESTATION_RETENTION_SLOTS));

        let protected_roots: HashSet<B256> = latest_known_aggregated_payloads_provider
            .iter_keys()?
            .into_iter()
            .map(|key| key.data_root)
            .chain(
                latest_new_aggregated_payloads_provider
                    .iter()?
                    .into_iter()
                    .map(|(key, _)| key.data_root),
            )
            .collect();

        let stale_roots: HashSet<B256> = attestation_data_by_root_provider
            .iter()?
            .into_iter()
            .filter(|(root, data)| {
                (data.target.slot <= finalized_slot || data.slot < cutoff_slot)
                    && !protected_roots.contains(root)
            })
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

fn build_running_votes_by_target_root(
    head_state: &LeanState,
) -> anyhow::Result<HashMap<B256, HashSet<u64>>> {
    let validator_count = head_state.validators.len();
    let mut votes_by_target_root = HashMap::new();

    for (root_index, target_root) in head_state.justifications_roots.iter().enumerate() {
        let mut voters = HashSet::new();
        for validator_index in 0..validator_count {
            let bit_index = root_index * validator_count + validator_index;
            if head_state
                .justifications_validators
                .get(bit_index)
                .map_err(|err| anyhow!("Failed to get justification vote bit: {err:?}"))?
            {
                voters.insert(validator_index as u64);
            }
        }
        votes_by_target_root.insert(*target_root, voters);
    }

    Ok(votes_by_target_root)
}

fn is_projected_slot_justified(
    justified_slots: &BitList<U262144>,
    finalized_slot: u64,
    candidate_slot: u64,
) -> bool {
    let Some(index) = justified_index_after(candidate_slot, finalized_slot) else {
        return candidate_slot <= finalized_slot;
    };

    index < justified_slots.len() as u64 && justified_slots.get(index as usize).unwrap_or(false)
}

fn extend_projected_justified_slots(
    justified_slots: &mut BitList<U262144>,
    finalized_slot: u64,
    target_slot: u64,
) -> anyhow::Result<()> {
    let Some(target_index) = justified_index_after(target_slot, finalized_slot) else {
        return Ok(());
    };
    let length = (target_index + 1) as usize;

    if justified_slots.len() < length {
        let new_bitlist = BitList::with_capacity(length)
            .map_err(|err| anyhow!("Failed to extend projected justified slots: {err:?}"))?;
        *justified_slots = new_bitlist.union(justified_slots);
    }

    Ok(())
}

fn set_projected_justified_slot(
    justified_slots: &mut BitList<U262144>,
    finalized_slot: u64,
    target_slot: u64,
) -> anyhow::Result<()> {
    extend_projected_justified_slots(justified_slots, finalized_slot, target_slot)?;

    if let Some(index) = justified_index_after(target_slot, finalized_slot) {
        justified_slots
            .set(index as usize, true)
            .map_err(|err| anyhow!("Failed to set projected justified slot: {err:?}"))?;
    }

    Ok(())
}

fn shift_projected_finalized_slot(
    justified_slots: &mut BitList<U262144>,
    finalized_slot: u64,
    new_finalized_slot: u64,
) -> anyhow::Result<()> {
    let delta = new_finalized_slot.saturating_sub(finalized_slot) as usize;
    if delta == 0 {
        return Ok(());
    }

    let new_len = justified_slots.len().saturating_sub(delta);
    let mut shifted = BitList::with_capacity(new_len)
        .map_err(|err| anyhow!("Failed to shift projected justified slots: {err:?}"))?;

    for index in delta..justified_slots.len() {
        if justified_slots.get(index).unwrap_or(false) {
            shifted
                .set(index - delta, true)
                .map_err(|err| anyhow!("Failed to set shifted justified slot: {err:?}"))?;
        }
    }

    *justified_slots = shifted;
    Ok(())
}

#[cfg(test)]
#[cfg(feature = "devnet5")]
mod tests {
    use alloy_primitives::B256;
    use ream_consensus_lean::{
        attestation::{AttestationData, MultiMessageAggregate, SignedAttestation},
        block::{Block, BlockBody, SignedBlock},
        checkpoint::Checkpoint,
    };
    use ream_post_quantum_crypto::leansig::signature::Signature;
    use ream_storage::tables::{field::REDBField, table::REDBTable};
    use ream_test_utils::store::sample_store;
    use ssz_types::VariableList;
    use tree_hash::TreeHash;

    use super::{BlockProductionStrategy, Store};

    async fn sample_store_as_store(no_of_validators: usize) -> Store {
        let test_store = sample_store(no_of_validators).await;
        Store {
            store: test_store.store,
            network_state: test_store.network_state,
            tick_interval_duration: None,
            block_production_strategy: BlockProductionStrategy::default(),
        }
    }

    fn fake_signed_block(slot: u64, proposer_index: u64, parent_root: B256) -> SignedBlock {
        SignedBlock {
            block: Block {
                slot,
                proposer_index,
                parent_root,
                state_root: B256::ZERO,
                body: BlockBody {
                    attestations: VariableList::empty(),
                },
            },
            proof: MultiMessageAggregate {
                proof: VariableList::default(),
            },
        }
    }

    async fn store_with_finalized_orphaned_branch() -> (Store, B256, B256, B256, B256) {
        let store = sample_store_as_store(10).await;

        let genesis_root = { store.store.lock().await.head_provider().get().unwrap() };

        let canonical_1 = fake_signed_block(1, 0, genesis_root);
        let canonical_1_root = canonical_1.block.tree_hash_root();
        let canonical_2 = fake_signed_block(2, 0, canonical_1_root);
        let canonical_2_root = canonical_2.block.tree_hash_root();

        let orphan_1 = fake_signed_block(1, 1, genesis_root);
        let orphan_1_root = orphan_1.block.tree_hash_root();
        let orphan_2 = fake_signed_block(2, 1, orphan_1_root);
        let orphan_2_root = orphan_2.block.tree_hash_root();

        {
            let db = store.store.lock().await;
            let block_provider = db.block_provider();
            block_provider
                .insert(canonical_1_root, canonical_1)
                .unwrap();
            block_provider
                .insert(canonical_2_root, canonical_2)
                .unwrap();
            block_provider.insert(orphan_1_root, orphan_1).unwrap();
            block_provider.insert(orphan_2_root, orphan_2).unwrap();

            db.latest_finalized_provider()
                .insert(Checkpoint {
                    root: canonical_2_root,
                    slot: 2,
                })
                .unwrap();
            db.time_provider().insert(1_000).unwrap();
        }

        (
            store,
            canonical_1_root,
            canonical_2_root,
            orphan_1_root,
            orphan_2_root,
        )
    }

    #[tokio::test]
    async fn test_validate_attestation_rejects_head_on_finalized_orphaned_branch() {
        let (store, _, _, orphan_1_root, orphan_2_root) =
            store_with_finalized_orphaned_branch().await;

        let genesis_root = { store.store.lock().await.head_provider().get().unwrap() };

        let attestation_data = AttestationData {
            slot: 2,
            head: Checkpoint {
                root: orphan_2_root,
                slot: 2,
            },
            target: Checkpoint {
                root: orphan_1_root,
                slot: 1,
            },
            source: Checkpoint {
                root: genesis_root,
                slot: 0,
            },
        };

        let err = store
            .validate_attestation(&SignedAttestation {
                validator_id: 0,
                message: attestation_data,
                signature: Signature::blank(),
            })
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Head checkpoint must descend from the finalized block"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn test_validate_attestation_accepts_head_descending_from_finalized() {
        let (store, canonical_1_root, canonical_2_root, _, _) =
            store_with_finalized_orphaned_branch().await;

        let genesis_root = { store.store.lock().await.head_provider().get().unwrap() };

        let attestation_data = AttestationData {
            slot: 2,
            head: Checkpoint {
                root: canonical_2_root,
                slot: 2,
            },
            target: Checkpoint {
                root: canonical_1_root,
                slot: 1,
            },
            source: Checkpoint {
                root: genesis_root,
                slot: 0,
            },
        };

        store
            .validate_attestation(&SignedAttestation {
                validator_id: 0,
                message: attestation_data,
                signature: Signature::blank(),
            })
            .await
            .unwrap();
    }
}
