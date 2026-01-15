use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
#[cfg(feature = "devnet2")]
use ream_consensus_lean::attestation::AggregatedAttestation;
#[cfg(feature = "devnet2")]
use ream_consensus_lean::attestation::AggregatedAttestations;
#[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
use ream_consensus_lean::attestation::Attestation;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAttestation},
    block::{Block, BlockBody, BlockWithSignatures, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
    state::LeanState,
    validator::is_proposer,
};
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
use ream_metrics::{
    ATTESTATION_VALIDATION_TIME, ATTESTATIONS_INVALID_TOTAL, ATTESTATIONS_VALID_TOTAL,
    FINALIZATIONS_TOTAL, FINALIZED_SLOT, FORK_CHOICE_BLOCK_PROCESSING_TIME, HEAD_SLOT,
    JUSTIFIED_SLOT, LATEST_FINALIZED_SLOT, LATEST_JUSTIFIED_SLOT, PROPOSE_BLOCK_TIME,
    SAFE_TARGET_SLOT, inc_int_counter_vec, set_int_gauge_vec, start_timer, stop_timer,
};
use ream_network_spec::networks::lean_network_spec;
use ream_network_state_lean::NetworkState;
use ream_post_quantum_crypto::leansig::signature::Signature;
use ream_storage::{
    db::lean::LeanDB,
    tables::{field::REDBField, table::REDBTable},
};
use ream_sync::rwlock::{Reader, Writer};
#[cfg(feature = "devnet2")]
use ssz_types::BitList;
use ssz_types::{VariableList, typenum::U4096};
use tokio::sync::Mutex;
use tree_hash::TreeHash;

use super::utils::is_justifiable_after;
use crate::constants::JUSTIFICATION_LOOKBACK_SLOTS;

pub type LeanStoreWriter = Writer<Store>;
pub type LeanStoreReader = Reader<Store>;

/// [Store] represents the state that the Lean node should maintain.
///
/// Most of the fields are based on the Python implementation of [`Staker`](https://github.com/ethereum/research/blob/d225a6775a9b184b5c1fd6c830cc58a375d9535f/3sf-mini/p2p.py#L15-L42),
/// but doesn't include `validator_id` as a node should manage multiple validators.
#[derive(Debug, Clone)]
pub struct Store {
    pub store: Arc<Mutex<LeanDB>>,
    pub network_state: Arc<NetworkState>,
}

impl Store {
    /// Initialize forkchoice store from an anchor state and anchor block.
    pub fn get_forkchoice_store(
        anchor_block: SignedBlockWithAttestation,
        anchor_state: LeanState,
        db: LeanDB,
        time: Option<u64>,
    ) -> anyhow::Result<Store> {
        ensure!(
            anchor_block.message.block.state_root == anchor_state.tree_hash_root(),
            "Anchor block state root must match anchor state hash"
        );
        let anchor_root = anchor_block.message.block.tree_hash_root();
        let anchor_slot = anchor_block.message.block.slot;
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

        Ok(Store {
            store: Arc::new(Mutex::new(db)),
            network_state: Arc::new(NetworkState::new(anchor_checkpoint, anchor_checkpoint)),
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
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            let mut current_root = attestation.message.data.head.root;

            #[cfg(feature = "devnet2")]
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

        let head_state = state_provider
            .get(head_provider.get()?)?
            .ok_or(anyhow!("Failed to get head state for safe target update"))?;

        let min_target_score = (head_state.validators.len() as u64 * 2).div_ceil(3);
        let latest_justified_root = latest_justified_provider.get()?.root;

        let (new_safe_target_root, new_safe_target_slot) = self
            .compute_lmd_ghost_head(
                latest_new_attestations_provider.iter_values()?,
                latest_justified_root,
                min_target_score,
            )
            .await?;

        safe_target_provider.insert(new_safe_target_root)?;

        // Update safe target slot metric
        set_int_gauge_vec(&SAFE_TARGET_SLOT, new_safe_target_slot as i64, &[]);

        Ok(())
    }

    /// Process new attestations that the staker has received. Attestation processing is done
    /// at a particular time, because of safe target and view merge rule
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

    /// Done upon processing new attestations or a new block
    pub async fn update_head(&self) -> anyhow::Result<()> {
        let (latest_known_attestations, latest_justified_provider, head_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_known_attestations_provider()
                    .get_all_attestations()?,
                db.latest_justified_provider(),
                db.head_provider(),
            )
        };

        let (new_head, new_head_slot) = self
            .compute_lmd_ghost_head(
                latest_known_attestations.into_values().map(Ok),
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
    pub async fn get_proposal_head(&self, slot: u64) -> anyhow::Result<B256> {
        let slot_time =
            lean_network_spec().genesis_time + slot * lean_network_spec().seconds_per_slot;
        self.on_tick(slot_time, true).await?;
        self.accept_new_attestations().await?;
        Ok(self.store.lock().await.head_provider().get()?)
    }

    pub async fn build_block(
        &self,
        slot: u64,
        proposer_index: u64,
        parent_root: B256,
        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))] attestations: Option<
            VariableList<Attestation, U4096>,
        >,
        #[cfg(feature = "devnet2")] attestations: Option<
            VariableList<AggregatedAttestations, U4096>,
        >,
    ) -> anyhow::Result<(Block, Vec<Signature>, LeanState)> {
        let (state_provider, latest_known_attestation_provider, block_provider) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.latest_known_attestations_provider(),
                db.block_provider(),
            )
        };
        let available_signed_attestations =
            latest_known_attestation_provider.get_all_attestations()?;
        let head_state = state_provider
            .get(parent_root)?
            .ok_or(anyhow!("State not found for head root"))?;
        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let mut attestations: VariableList<Attestation, U4096> =
            attestations.unwrap_or_else(VariableList::empty);

        #[cfg(feature = "devnet2")]
        let mut attestations: VariableList<AggregatedAttestations, U4096> =
            attestations.unwrap_or_else(VariableList::empty);

        let mut signatures: Vec<Signature> = Vec::new();

        let (mut candidate_block, signatures, post_state) = loop {
            #[cfg(feature = "devnet2")]
            let attestations_list: VariableList<AggregatedAttestation, U4096> = {
                let mut groups: HashMap<AttestationData, Vec<u64>> = HashMap::new();
                for attestation in attestations.iter() {
                    groups
                        .entry(attestation.data.clone())
                        .or_default()
                        .push(attestation.validator_id);
                }

                VariableList::new(
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
                .map_err(|err| anyhow!("Limit exceeded: {err:?}"))?
            };
            let candidate_block = Block {
                slot,
                proposer_index,
                parent_root,
                state_root: B256::ZERO,
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                body: BlockBody {
                    attestations: attestations.clone(),
                },
                #[cfg(feature = "devnet2")]
                body: BlockBody {
                    attestations: attestations_list.clone(),
                },
            };
            let mut advanced_state = head_state.clone();
            advanced_state.process_slots(slot)?;
            advanced_state.process_block(&candidate_block)?;
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            let mut new_attestations: VariableList<Attestation, U4096> = VariableList::empty();
            #[cfg(feature = "devnet2")]
            let mut new_attestations: VariableList<AggregatedAttestations, U4096> =
                VariableList::empty();
            let mut new_signatures: Vec<Signature> = Vec::new();
            for signed_attestation in available_signed_attestations.values() {
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                let data = &signed_attestation.message.data;
                #[cfg(feature = "devnet2")]
                let data = &signed_attestation.message;
                #[cfg(feature = "devnet2")]
                let attestation = AggregatedAttestations {
                    validator_id: signed_attestation.validator_id,
                    data: data.clone(),
                };

                if !block_provider.contains_key(data.head.root) {
                    continue;
                }
                if data.source != advanced_state.latest_justified {
                    continue;
                }
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                if !attestations.contains(&signed_attestation.message) {
                    new_attestations
                        .push(signed_attestation.message.clone())
                        .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
                    new_signatures.push(signed_attestation.signature);
                }

                #[cfg(feature = "devnet2")]
                if !attestations.contains(&attestation) {
                    new_attestations
                        .push(attestation)
                        .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
                    new_signatures.push(signed_attestation.signature);
                }
            }
            if new_attestations.is_empty() {
                break (candidate_block, signatures, advanced_state);
            }

            for attestation in new_attestations {
                attestations
                    .push(attestation)
                    .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
            }

            for signature in new_signatures {
                signatures.push(signature);
            }
        };

        candidate_block.state_root = post_state.tree_hash_root();
        Ok((candidate_block, signatures, post_state))
    }

    pub async fn produce_block_with_signatures(
        &self,
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
        let (mut candidate_block, signatures, post_state) = self
            .build_block(slot, validator_index, head_root, None)
            .await?;

        stop_timer(add_attestations_timer);

        let compute_state_root_timer = start_timer(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        candidate_block.state_root = post_state.tree_hash_root();
        stop_timer(compute_state_root_timer);
        Ok(BlockWithSignatures {
            block: candidate_block,
            signatures: VariableList::new(signatures)
                .map_err(|err| anyhow!("Failed to return signatures {err:?}"))?,
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
        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let signatures = &signed_block_with_attestation.signature;
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

        let latest_finalized =
            if parent_state.latest_finalized.slot > latest_finalized_provider.get()?.slot {
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

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        for (attestation, signature) in signed_block_with_attestation
            .message
            .block
            .body
            .attestations
            .iter()
            .zip(signed_block_with_attestation.signature.clone())
        {
            self.on_attestation(
                SignedAttestation {
                    message: attestation.clone(),
                    signature,
                },
                true,
            )
            .await?;
        }

        #[cfg(feature = "devnet2")]
        {
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

            for (aggregated_attestation, aggregated_signature) in aggregated_attestations
                .into_iter()
                .zip(attestation_signatures)
            {
                let validator_ids: Vec<u64> = aggregated_attestation
                    .aggregation_bits
                    .iter()
                    .enumerate()
                    .filter(|(_, bit)| *bit)
                    .map(|(index, _)| index as u64)
                    .collect();

                ensure!(
                    validator_ids.len() == aggregated_signature.inner.len(),
                    "Aggregated attestation signature count mismatch"
                );

                for (validator_id, signature) in
                    validator_ids.into_iter().zip(attestation_signatures)
                {
                    self.on_attestation(
                        SignedAttestation {
                            validator_id,
                            message: aggregated_attestation.message.clone(),
                            signature: *signature,
                        },
                        true,
                    )
                    .await?;
                }
            }
        }

        self.update_head().await?;

        self.on_attestation(
            SignedAttestation {
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                message: proposer_attestation.clone(),
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                signature: *signatures
                    .get(block.body.attestations.len())
                    .ok_or(anyhow!("Failed to get attestation"))?,
                #[cfg(feature = "devnet2")]
                message: proposer_attestation.data.clone(),
                #[cfg(feature = "devnet2")]
                signature: signed_block_with_attestation.signature.proposer_signature,
                #[cfg(feature = "devnet2")]
                validator_id: proposer_attestation.validator_id,
            },
            false,
        )
        .await?;

        stop_timer(block_processing_timer);
        Ok(())
    }

    pub async fn validate_attestation(
        &self,
        signed_attestation: &SignedAttestation,
    ) -> anyhow::Result<()> {
        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let data = &signed_attestation.message.data;
        #[cfg(feature = "devnet2")]
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

    pub async fn on_attestation(
        &self,
        signed_attestation: SignedAttestation,
        is_from_block: bool,
    ) -> anyhow::Result<()> {
        let (latest_known_attestations_provider, latest_new_attestations_provider, time_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_known_attestations_provider(),
                db.latest_new_attestations_provider(),
                db.time_provider(),
            )
        };

        let validate_attestation_timer = start_timer(&ATTESTATION_VALIDATION_TIME, &[]);

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

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let validator_id = signed_attestation.message.validator_id;
        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation_slot = signed_attestation.message.data.slot;

        #[cfg(feature = "devnet2")]
        let validator_id = signed_attestation.validator_id;
        #[cfg(feature = "devnet2")]
        let attestation_slot = signed_attestation.message.slot;

        if is_from_block {
            let latest_known = match latest_known_attestations_provider.get(validator_id)? {
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                Some(latest_known) => latest_known.message.data.slot < attestation_slot,
                #[cfg(feature = "devnet2")]
                Some(latest_known) => latest_known.message.slot < attestation_slot,
                None => true,
            };
            if latest_known {
                latest_known_attestations_provider.insert(validator_id, signed_attestation)?;
            }
            let remove = match latest_new_attestations_provider.get(validator_id)? {
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                Some(new_new) => new_new.message.data.slot <= attestation_slot,
                #[cfg(feature = "devnet2")]
                Some(new_new) => new_new.message.slot <= attestation_slot,
                None => false,
            };
            if remove {
                latest_new_attestations_provider.remove(validator_id)?;
            }
        } else {
            let time_slots = time_provider.get()? / lean_network_spec().seconds_per_slot;
            ensure!(
                attestation_slot <= time_slots,
                "Attestation from future slot {attestation_slot} <= {time_slots}",
            );
            let latest_new = match latest_new_attestations_provider.get(validator_id)? {
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                Some(latest_new) => latest_new.message.data.slot < attestation_slot,
                #[cfg(feature = "devnet2")]
                Some(latest_new) => latest_new.message.slot < attestation_slot,
                None => true,
            };
            if latest_new {
                latest_new_attestations_provider.insert(validator_id, signed_attestation)?;
            }
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
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{B256, FixedBytes};
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    use ream_consensus_lean::attestation::Attestation;
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    use ream_consensus_lean::block::{BlockBody, BlockHeader};
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    use ream_consensus_lean::config::Config;
    #[cfg(feature = "devnet2")]
    use ream_consensus_lean::{
        attestation::{AggregatedAttestation, AggregatedAttestations},
        block::BlockSignatures,
    };
    use ream_consensus_lean::{
        attestation::{AttestationData, SignedAttestation},
        block::{Block, BlockWithAttestation, BlockWithSignatures, SignedBlockWithAttestation},
        checkpoint::Checkpoint,
        state::LeanState,
        utils::generate_default_validators,
        validator::is_proposer,
    };
    use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
    use ream_network_spec::networks::{LeanNetworkSpec, lean_network_spec, set_lean_network_spec};
    use ream_post_quantum_crypto::leansig::signature::Signature;
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
            )
            .unwrap(),
            genesis_state,
        )
    }

    pub fn build_signed_block_with_attestation(
        attestation_data: AttestationData,
        block: Block,
        mut signatures: VariableList<Signature, U4096>,
    ) -> SignedBlockWithAttestation {
        signatures.push(Signature::blank()).unwrap();
        SignedBlockWithAttestation {
            message: BlockWithAttestation {
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                proposer_attestation: Attestation {
                    validator_id: block.proposer_index,
                    data: attestation_data,
                },
                #[cfg(feature = "devnet2")]
                proposer_attestation: AggregatedAttestations {
                    validator_id: block.proposer_index,
                    data: attestation_data,
                },
                block,
            },
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            signature: signatures,
            #[cfg(feature = "devnet2")]
            signature: BlockSignatures {
                attestation_signatures: signatures,
                proposer_signature: Signature::blank(),
            },
        }
    }

    // BLOCK PRODUCTION TESTS

    /// Test basic block production by authorized proposer.
    #[tokio::test]
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    async fn test_produce_block_basic() {
        let (mut store, mut genesis_state) = sample_store(10).await;

        genesis_state.process_slots(1).unwrap();
        let store_head = store.store.lock().await.head_provider().get().unwrap();

        let (block_provider, state_provider) = {
            let store = store.store.lock().await;
            (store.block_provider(), store.state_provider())
        };

        let BlockWithSignatures { block, signatures } =
            store.produce_block_with_signatures(1, 1).await.unwrap();

        assert_eq!(block.slot, 1);
        assert_eq!(block.proposer_index, 1);
        assert_eq!(block.parent_root, store_head);
        assert_ne!(block.state_root, B256::ZERO);

        let signed_block_with_attestation = build_signed_block_with_attestation(
            store.produce_attestation_data(1).await.unwrap(),
            block.clone(),
            signatures,
        );

        store
            .on_block(&signed_block_with_attestation, false)
            .await
            .unwrap();
        let block_hash = block.tree_hash_root();
        assert!(block_provider.get(block_hash).unwrap().is_some());
        assert!(state_provider.get(block_hash).unwrap().is_some());
    }

    /// Test block production fails for unauthorized proposer.
    #[tokio::test]
    async fn test_produce_block_unauthorized_proposer() {
        let (store, _) = sample_store(10).await;
        let block_with_signature = store.produce_block_with_signatures(1, 2).await;
        assert!(block_with_signature.is_err());
    }

    /// Test block production includes available attestations.
    #[tokio::test]
    async fn test_produce_block_with_attestations() {
        let (store, _) = sample_store(10).await;

        let (head_provider, block_provider, justified_provider, latest_known_attestations) = {
            let db = store.store.lock().await;
            (
                db.head_provider(),
                db.block_provider(),
                db.latest_justified_provider(),
                db.latest_known_attestations_provider(),
            )
        };
        let head = head_provider.get().unwrap();
        let head_block = block_provider.get(head).unwrap().unwrap();
        let justified_checkpoint = justified_provider.get().unwrap();
        let attestation_target = store.get_attestation_target().await.unwrap();

        let attestation_1 = SignedAttestation {
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            message: Attestation {
                validator_id: 5,
                data: AttestationData {
                    slot: head_block.message.block.slot,
                    head: Checkpoint {
                        root: head,
                        slot: head_block.message.block.slot,
                    },
                    target: justified_checkpoint,
                    source: attestation_target,
                },
            },
            #[cfg(feature = "devnet2")]
            message: AttestationData {
                slot: head_block.message.block.slot,
                head: Checkpoint {
                    root: head,
                    slot: head_block.message.block.slot,
                },
                target: justified_checkpoint,
                source: attestation_target,
            },
            signature: Signature::blank(),
            #[cfg(feature = "devnet2")]
            validator_id: 5,
        };

        let attestation_2 = SignedAttestation {
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            message: Attestation {
                validator_id: 6,
                data: AttestationData {
                    slot: head_block.message.block.slot,
                    head: Checkpoint {
                        root: head,
                        slot: head_block.message.block.slot,
                    },
                    target: justified_checkpoint,
                    source: attestation_target,
                },
            },
            #[cfg(feature = "devnet2")]
            message: AttestationData {
                slot: head_block.message.block.slot,
                head: Checkpoint {
                    root: head,
                    slot: head_block.message.block.slot,
                },
                target: justified_checkpoint,
                source: attestation_target,
            },
            signature: Signature::blank(),
            #[cfg(feature = "devnet2")]
            validator_id: 5,
        };
        latest_known_attestations
            .batch_insert([(5, attestation_1), (6, attestation_2)])
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
        let (store, mut genesis_state) = sample_store(10).await;
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
        let (store, _) = sample_store(10).await;
        let head = store.get_proposal_head(3).await.unwrap();

        let BlockWithSignatures { block, .. } =
            store.produce_block_with_signatures(3, 3).await.unwrap();

        assert_eq!(block.body.attestations.len(), 0);
        assert_eq!(block.slot, 3);
        assert_eq!(block.parent_root, head);
        assert!(!block.state_root.is_zero());
    }

    /// Test that produced block's state is consistent with block content
    #[tokio::test]
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    pub async fn test_produce_block_state_consistency() {
        let (mut store, _) = sample_store(10).await;

        let head = store.get_proposal_head(3).await.unwrap();
        let (block_provider, state_provider, latest_known_attestations, latest_justified_provider) = {
            let store = store.store.lock().await;
            (
                store.block_provider(),
                store.state_provider(),
                store.latest_known_attestations_provider(),
                store.latest_justified_provider(),
            )
        };
        let head_block = block_provider.get(head).unwrap().unwrap();

        let attestation = SignedAttestation {
            message: Attestation {
                validator_id: 7,
                data: AttestationData {
                    slot: head_block.message.block.slot,
                    head: Checkpoint {
                        root: head,
                        slot: head_block.message.block.slot,
                    },
                    target: latest_justified_provider.get().unwrap(),
                    source: store.get_attestation_target().await.unwrap(),
                },
            },
            signature: Signature::blank(),
        };
        latest_known_attestations.insert(7, attestation).unwrap();

        let BlockWithSignatures { block, signatures } =
            store.produce_block_with_signatures(4, 4).await.unwrap();

        let signed_block_with_attestation = build_signed_block_with_attestation(
            store.produce_attestation_data(4).await.unwrap(),
            block.clone(),
            signatures,
        );

        store
            .on_block(&signed_block_with_attestation, false)
            .await
            .unwrap();

        assert_eq!(
            block.state_root,
            state_provider
                .get(block.tree_hash_root())
                .unwrap()
                .unwrap()
                .tree_hash_root()
        );
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

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation = Attestation {
            validator_id,
            data: store.produce_attestation_data(slot).await.unwrap(),
        };

        #[cfg(feature = "devnet2")]
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

        let (store, _) = sample_store(10).await;
        let block_provider = store.store.lock().await.block_provider();

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation = Attestation {
            validator_id: 8,
            data: store.produce_attestation_data(slot).await.unwrap(),
        };

        #[cfg(feature = "devnet2")]
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
        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation = Attestation {
            validator_id: 9,
            data: store.produce_attestation_data(3).await.unwrap(),
        };

        #[cfg(feature = "devnet2")]
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
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            let attestation = Attestation {
                validator_id,
                data: store.produce_attestation_data(slot).await.unwrap(),
            };
            #[cfg(feature = "devnet2")]
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
        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let validator_id = 3;

        let (store, _) = sample_store(10).await;
        let latest_justified_provider = store.store.lock().await.latest_justified_provider();

        #[cfg(feature = "devnet2")]
        let mut aggregation_bits = BitList::<U4096>::with_capacity(32).unwrap();
        #[cfg(feature = "devnet2")]
        aggregation_bits.set(0, true).unwrap();

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation_1 = Attestation {
            validator_id,
            data: store.produce_attestation_data(1).await.unwrap(),
        };

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation_2 = Attestation {
            validator_id,
            data: store.produce_attestation_data(2).await.unwrap(),
        };

        #[cfg(feature = "devnet2")]
        let attestation_1 = AggregatedAttestation {
            aggregation_bits: aggregation_bits.clone(),
            message: store.produce_attestation_data(1).await.unwrap(),
        };

        #[cfg(feature = "devnet2")]
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

        #[cfg(feature = "devnet2")]
        let mut aggregation_bits = BitList::<U4096>::with_capacity(32).unwrap();
        #[cfg(feature = "devnet2")]
        aggregation_bits.set(0, true).unwrap();

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation = Attestation {
            validator_id: 2,
            data: store.produce_attestation_data(5).await.unwrap(),
        };

        #[cfg(feature = "devnet2")]
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

    // VALIDATOR INTEGRATION TESTS

    /// Test producing a block then creating attestation for it.
    #[tokio::test]
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    pub async fn test_block_production_then_attestation() {
        let (mut store, _) = sample_store(10).await;

        let latest_justified_provider = {
            let store = store.store.lock().await;
            store.latest_justified_provider()
        };
        let BlockWithSignatures { block, signatures } =
            store.produce_block_with_signatures(1, 1).await.unwrap();

        let signed_block_with_attestation = build_signed_block_with_attestation(
            store.produce_attestation_data(1).await.unwrap(),
            block.clone(),
            signatures,
        );

        store
            .on_block(&signed_block_with_attestation, false)
            .await
            .unwrap();

        store.update_head().await.unwrap();

        let attestation = Attestation {
            validator_id: 7,
            data: store.produce_attestation_data(2).await.unwrap(),
        };

        assert_eq!(attestation.validator_id, 7);
        assert_eq!(attestation.slot(), 2);
        assert_eq!(
            attestation.data.source,
            latest_justified_provider.get().unwrap()
        );
    }

    /// Test producing a block then creating attestation for it.
    #[tokio::test]
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    pub async fn test_multiple_validators_coordination() {
        let (mut store, _) = sample_store(10).await;

        let (block_provider, head_provider) = {
            let store = store.store.lock().await;
            (store.block_provider(), store.head_provider())
        };

        let genesis_hash = head_provider.get().unwrap();
        let BlockWithSignatures { block, signatures } =
            store.produce_block_with_signatures(1, 1).await.unwrap();

        let signed_block_with_attestation_1 = build_signed_block_with_attestation(
            store.produce_attestation_data(1).await.unwrap(),
            block.clone(),
            signatures,
        );

        store
            .on_block(&signed_block_with_attestation_1, false)
            .await
            .unwrap();

        let block_1_hash = block.tree_hash_root();
        assert!(block_provider.get(block_1_hash).unwrap().is_some());

        let mut attestations = Vec::new();
        for i in 2..6 {
            let attestation = Attestation {
                validator_id: i,
                data: store.produce_attestation_data(2).await.unwrap(),
            };

            attestations.push(attestation);
        }

        let first_attestation = &attestations[0];
        for attestation in attestations.iter().skip(1) {
            assert_eq!(attestation.data.head, first_attestation.data.head);
            assert_eq!(attestation.data.target, first_attestation.data.target);
            assert_eq!(attestation.data.source, first_attestation.data.source);
        }

        let BlockWithSignatures { block, signatures } =
            store.produce_block_with_signatures(2, 2).await.unwrap();

        let signed_block_with_attestation_2 = build_signed_block_with_attestation(
            store.produce_attestation_data(2).await.unwrap(),
            block.clone(),
            signatures,
        );
        store
            .on_block(&signed_block_with_attestation_2, false)
            .await
            .unwrap();

        assert_eq!(
            signed_block_with_attestation_2.message.block.proposer_index,
            2
        );
        assert_eq!(signed_block_with_attestation_2.message.block.slot, 2);

        assert!(
            block_provider
                .get(block.tree_hash_root())
                .unwrap()
                .is_some()
        );

        assert_eq!(
            signed_block_with_attestation_1.message.block.parent_root,
            genesis_hash
        );
        assert_eq!(
            signed_block_with_attestation_2.message.block.parent_root,
            block_1_hash
        );
    }

    /// Test edge cases in validator operations.
    #[tokio::test]
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    pub async fn test_validator_edge_cases() {
        let (mut store, _) = sample_store(10).await;

        let BlockWithSignatures { block, signatures } =
            store.produce_block_with_signatures(9, 9).await.unwrap();
        assert_eq!(block.proposer_index, 9);

        let signed_block_with_attestation_1 = build_signed_block_with_attestation(
            store.produce_attestation_data(9).await.unwrap(),
            block.clone(),
            signatures,
        );

        store
            .on_block(&signed_block_with_attestation_1, false)
            .await
            .unwrap();

        let attestation = Attestation {
            validator_id: 9,
            data: store.produce_attestation_data(10).await.unwrap(),
        };

        assert_eq!(attestation.validator_id, 9);
        assert_eq!(attestation.slot(), 10);
    }

    /// Test validator operations with minimal store state.
    #[tokio::test]
    #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
    pub async fn test_validator_operations_empty_store() {
        let empty_checkpoint = Checkpoint {
            slot: 0,
            root: B256::ZERO,
        };
        let genesis_state = LeanState {
            config: Config { genesis_time: 1000 },
            slot: 0,
            latest_block_header: BlockHeader {
                slot: 0,
                proposer_index: 0,
                parent_root: B256::ZERO,
                state_root: B256::ZERO,
                body_root: BlockBody {
                    attestations: Default::default(),
                }
                .tree_hash_root(),
            },
            latest_justified: empty_checkpoint,
            latest_finalized: empty_checkpoint,
            historical_block_hashes: VariableList::empty(),
            justified_slots: BitList::with_capacity(0).unwrap(),
            validators: VariableList::try_from(generate_default_validators(3)).unwrap(),
            justifications_roots: VariableList::empty(),
            justifications_validators: BitList::with_capacity(0).unwrap(),
        };

        let genesis_block = Block {
            slot: 0,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root: genesis_state.tree_hash_root(),
            body: BlockBody {
                attestations: Default::default(),
            },
        };

        let final_checkpoint = Checkpoint {
            slot: 0,
            root: genesis_block.tree_hash_root(),
        };

        set_lean_network_spec(LeanNetworkSpec::ephemery().into());

        let mut store = Store::get_forkchoice_store(
            build_signed_block_with_attestation(
                AttestationData {
                    slot: genesis_block.slot,
                    head: final_checkpoint,
                    target: final_checkpoint,
                    source: final_checkpoint,
                },
                genesis_block,
                VariableList::default(),
            ),
            genesis_state,
            db_setup(),
            None,
        )
        .unwrap();

        let BlockWithSignatures { block, signatures } =
            store.produce_block_with_signatures(1, 1).await.unwrap();

        let signed_block_with_attestation_1 = build_signed_block_with_attestation(
            store.produce_attestation_data(1).await.unwrap(),
            block.clone(),
            signatures,
        );

        assert!(
            store
                .on_block(&signed_block_with_attestation_1, false)
                .await
                .is_ok()
        );
        assert!(store.produce_attestation_data(1).await.is_ok());
    }

    // VALIDATOR ERROR HANDLING TESTS

    /// Test error when wrong validator tries to produce block.
    #[tokio::test]
    pub async fn test_produce_block_wrong_proposer() {
        let (store, _) = sample_store(10).await;

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
        let (store, _) = sample_store(10).await;
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

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        let attestation = Attestation {
            validator_id: 1000000,
            data: store.produce_attestation_data(1).await.unwrap(),
        };

        #[cfg(feature = "devnet2")]
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
        let (store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = state.config.genesis_time + 200;

        store.on_tick(target_time, true).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time > initial_time);
    }

    // Test on_tick without proposal.
    #[tokio::test]
    pub async fn test_on_tick_no_proposal() {
        let (store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = state.config.genesis_time + 100;

        store.on_tick(target_time, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    // Test on_tick when already at target time.
    #[tokio::test]
    pub async fn test_on_tick_already_current() {
        let (store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let current_target = state.config.genesis_time + initial_time;

        store.on_tick(current_target, true).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time - initial_time <= 10);
    }

    // Test on_tick with small time increment.
    #[tokio::test]
    pub async fn test_on_tick_small_increment() {
        let (store, state) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();
        let target_time = state.config.genesis_time + initial_time + 1;

        store.on_tick(target_time, false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    // TEST INTERVAL TICKING

    // Test basic interval ticking.
    #[tokio::test]
    pub async fn test_tick_interval_basic() {
        let (store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        store.tick_interval(false).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 1)
    }

    // Test interval ticking with proposal.
    #[tokio::test]
    pub async fn test_tick_interval_with_proposal() {
        let (store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        store.tick_interval(true).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 1)
    }

    // Test sequence of interval ticks.
    #[tokio::test]
    pub async fn test_tick_interval_sequence() {
        let (store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        for i in 0..5 {
            store.tick_interval((i % 2) == 0).await.unwrap();
        }

        let new_time = time_provider.get().unwrap();

        assert!(new_time == initial_time + 5)
    }

    // Test different actions performed based on interval phase.
    #[tokio::test]
    pub async fn test_tick_interval_actions_by_phase() {
        let (store, _) = sample_store(10).await;

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
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                message: Attestation {
                    validator_id: 5,
                    data: AttestationData {
                        slot: 1,
                        head: justified_checkpoint,
                        target: test_checkpoint,
                        source: justified_checkpoint,
                    },
                },
                #[cfg(feature = "devnet2")]
                message: AttestationData {
                    slot: 1,
                    head: justified_checkpoint,
                    target: test_checkpoint,
                    source: justified_checkpoint,
                },
                #[cfg(feature = "devnet2")]
                validator_id: 5,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            db_table
                .insert(signed_attestation.message.validator_id, signed_attestation)
                .unwrap();

            #[cfg(feature = "devnet2")]
            db_table
                .insert(signed_attestation.validator_id, signed_attestation)
                .unwrap();
        };

        for interval in 0..INTERVALS_PER_SLOT {
            let has_proposal = interval == 0;
            store.tick_interval(has_proposal).await.unwrap();

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
        let (store, _) = sample_store(10).await;

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
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                message: Attestation {
                    validator_id: 5,
                    data: AttestationData {
                        slot: 1,
                        head: justified_checkpoint,
                        target: checkpoint,
                        source: justified_checkpoint,
                    },
                },
                #[cfg(feature = "devnet2")]
                message: AttestationData {
                    slot: 1,
                    head: justified_checkpoint,
                    target: checkpoint,
                    source: justified_checkpoint,
                },
                #[cfg(feature = "devnet2")]
                validator_id: 5,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            db_table
                .insert(signed_attestation.message.validator_id, signed_attestation)
                .unwrap();

            #[cfg(feature = "devnet2")]
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
        let (store, _) = sample_store(10).await;

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
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                message: Attestation {
                    validator_id: i,
                    data: AttestationData {
                        slot: i,
                        head: justified_checkpoint,
                        target: *checkpoint,
                        source: justified_checkpoint,
                    },
                },
                #[cfg(feature = "devnet2")]
                message: AttestationData {
                    slot: i,
                    head: justified_checkpoint,
                    target: *checkpoint,
                    source: justified_checkpoint,
                },
                #[cfg(feature = "devnet2")]
                validator_id: i,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            db_table
                .insert(signed_attestation.message.validator_id, signed_attestation)
                .unwrap();

            #[cfg(feature = "devnet2")]
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

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
        for (i, checkpoint) in checkpoints.iter().enumerate().map(|(i, c)| (i as u64, c)) {
            let stored_checkpoint = latest_known_attestations_provider
                .get(i)
                .unwrap()
                .unwrap()
                .message
                .data
                .target;
            assert!(stored_checkpoint == *checkpoint);
        }

        #[cfg(feature = "devnet2")]
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
        let (store, _) = sample_store(10).await;

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
        let (store, _) = sample_store(10).await;

        let head = store.get_proposal_head(0).await.unwrap();

        let stored_head = { store.store.lock().await.head_provider().get().unwrap() };

        assert!(head == stored_head);
    }

    // Test that get_proposal_head advances store time appropriately.
    #[tokio::test]
    pub async fn test_get_proposal_head_advances_time() {
        let (store, _) = sample_store(10).await;
        let time_provider = { store.store.lock().await.time_provider() };

        let initial_time = time_provider.get().unwrap();

        store.get_proposal_head(5).await.unwrap();

        let new_time = time_provider.get().unwrap();

        assert!(new_time >= initial_time);
    }

    // Test that get_proposal_head processes pending attestations.
    #[tokio::test]
    pub async fn test_get_proposal_head_processes_attestations() {
        let (store, _) = sample_store(10).await;

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
                #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
                message: Attestation {
                    validator_id: 10,
                    data: AttestationData {
                        slot: 10,
                        head: justified_checkpoint,
                        target: checkpoint,
                        source: justified_checkpoint,
                    },
                },
                #[cfg(feature = "devnet2")]
                message: AttestationData {
                    slot: 10,
                    head: justified_checkpoint,
                    target: checkpoint,
                    source: justified_checkpoint,
                },
                #[cfg(feature = "devnet2")]
                validator_id: 10,
                signature: Signature::blank(),
            };
            let db_table = db.latest_new_attestations_provider();
            #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
            db_table
                .insert(signed_attestation.message.validator_id, signed_attestation)
                .unwrap();

            #[cfg(feature = "devnet2")]
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

        #[cfg(all(feature = "devnet1", not(feature = "devnet2")))]
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
                .data
                .target
        };

        #[cfg(feature = "devnet2")]
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
