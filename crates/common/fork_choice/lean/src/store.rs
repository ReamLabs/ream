use std::{collections::HashMap, sync::Arc};

use alloy_primitives::{B256, FixedBytes};
use anyhow::{anyhow, ensure};
use ream_consensus_lean::{
    attestation::{Attestation, AttestationData, SignedAttestation},
    block::{Block, BlockBody, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
    state::LeanState,
    validator::is_proposer,
};
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
use ream_metrics::{HEAD_SLOT, PROPOSE_BLOCK_TIME, set_int_gauge_vec, start_timer_vec, stop_timer};
use ream_network_spec::networks::lean_network_spec;
use ream_storage::{
    db::lean::LeanDB,
    tables::{field::REDBField, table::REDBTable},
};
use ream_sync::rwlock::{Reader, Writer};
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
    /// Database.
    pub store: Arc<Mutex<LeanDB>>,
}

impl Store {
    /// Initialize forkchoice store from an anchor state and anchor block.
    pub fn get_forkchoice_store(
        anchor_block: SignedBlockWithAttestation,
        anchor_state: LeanState,
        db: LeanDB,
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
        db.lean_time_provider()
            .insert(anchor_slot * lean_network_spec().seconds_per_slot)
            .expect("Failed to insert anchor slot");
        db.lean_block_provider()
            .insert(anchor_root, anchor_block)
            .expect("Failed to insert genesis block");
        db.latest_finalized_provider()
            .insert(anchor_checkpoint)
            .expect("Failed to insert latest finalized checkpoint");
        db.latest_justified_provider()
            .insert(anchor_checkpoint)
            .expect("Failed to insert latest justified checkpoint");
        db.lean_state_provider()
            .insert(anchor_root, anchor_state)
            .expect("Failed to insert genesis state");
        db.lean_head_provider()
            .insert(anchor_root)
            .expect("Failed to insert genesis block hash");
        db.lean_safe_target_provider()
            .insert(anchor_root)
            .expect("Failed to insert genesis block hash");

        Ok(Store {
            store: Arc::new(Mutex::new(db)),
        })
    }

    /// Use LMD GHOST to get the head, given a particular root (usually the
    /// latest known justified block)
    pub async fn get_fork_choice_head(
        &self,
        latest_votes: impl Iterator<Item = anyhow::Result<SignedAttestation>>,
        provided_root: B256,
        min_score: u64,
    ) -> anyhow::Result<B256> {
        let mut root = provided_root;

        let (slot_index_table, lean_block_provider) = {
            let db = self.store.lock().await;
            (db.slot_index_provider(), db.lean_block_provider())
        };

        // Start at genesis by default
        if root == B256::ZERO {
            root = slot_index_table
                .get_oldest_root()?
                .ok_or(anyhow!("No blocks found to calculate fork choice"))?;
        }

        // For each block, count the number of votes for that block. A vote
        // for any descendant of a block also counts as a vote for that block
        let mut vote_weights = HashMap::<B256, u64>::new();

        for signed_vote in latest_votes {
            let signed_vote = signed_vote?;
            if lean_block_provider.contains_key(signed_vote.message.head().root) {
                let mut block_hash = signed_vote.message.head().root;
                while {
                    let current_block = lean_block_provider
                        .get(block_hash)?
                        .ok_or_else(|| anyhow!("Block not found for vote head: {block_hash}"))?
                        .message
                        .block;
                    let root_block = lean_block_provider
                        .get(root)?
                        .ok_or_else(|| anyhow!("Block not found for root: {root}"))?
                        .message
                        .block;
                    current_block.slot > root_block.slot
                } {
                    let current_weights = vote_weights.get(&block_hash).unwrap_or(&0);
                    vote_weights.insert(block_hash, current_weights + 1);
                    block_hash = lean_block_provider
                        .get(block_hash)?
                        .map(|block| block.message.block.parent_root)
                        .ok_or_else(|| anyhow!("Block not found for block parent: {block_hash}"))?;
                }
            }
        }

        // Identify the children of each block
        let children_map = lean_block_provider.get_children_map(min_score, &vote_weights)?;

        // Start at the root (latest justified hash or genesis) and repeatedly
        // choose the child with the most latest votes, tiebreaking by slot then hash
        let mut current_root = root;

        while let Some(children) = children_map.get(&current_root) {
            current_root = *children
                .iter()
                .max_by_key(|child_hash| {
                    let vote_weight = vote_weights.get(*child_hash).unwrap_or(&0);
                    let slot = lean_block_provider
                        .get(**child_hash)
                        .map(|maybe_block| match maybe_block {
                            Some(block) => block.message.block.slot,
                            None => 0,
                        })
                        .unwrap_or(0);
                    (*vote_weight, slot, *(*child_hash))
                })
                .ok_or_else(|| anyhow!("No children found for current root: {current_root}"))?;
        }

        Ok(current_root)
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
    ///
    /// See lean specification:
    /// https://github.com/leanEthereum/leanSpec/blob/f8e8d271d8b8b6513d34c78692aff47438d6fa18/src/lean_spec/subspecs/forkchoice/store.py#L301-L317
    pub async fn update_safe_target(&self) -> anyhow::Result<()> {
        // 2/3rd majority min voting weight for target selection
        // Note that we use ceiling division here.
        let (
            head_provider,
            state_provider,
            latest_justified_provider,
            lean_safe_target_provider,
            lean_latest_new_attestations_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.lean_head_provider(),
                db.lean_state_provider(),
                db.latest_justified_provider(),
                db.lean_safe_target_provider(),
                db.lean_latest_new_attestations_provider(),
            )
        };

        let head_state = state_provider
            .get(head_provider.get()?)?
            .ok_or(anyhow!("Failed to get head state for safe target update"))?;

        let min_target_score = (head_state.validators.len() as u64 * 2).div_ceil(3);
        let latest_justified_root = latest_justified_provider.get()?.root;

        lean_safe_target_provider.insert(
            self.get_fork_choice_head(
                lean_latest_new_attestations_provider.iter_values()?,
                latest_justified_root,
                min_target_score,
            )
            .await?,
        )?;

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
                .lean_latest_new_attestations_provider()
                .drain()?
                .into_iter(),
        )?;

        self.update_head().await?;
        Ok(())
    }

    pub async fn tick_interval(&self, has_proposal: bool) -> anyhow::Result<()> {
        let current_interval = {
            let time_provider = self.store.lock().await.lean_time_provider();
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
        let tick_interval_time = (time - lean_network_spec().genesis_time)
            / lean_network_spec().seconds_per_slot
            / seconds_per_interval;

        let time_provider = self.store.lock().await.lean_time_provider();
        while time_provider.get()? < tick_interval_time {
            let should_signal_proposal =
                has_proposal && (time_provider.get()? + 1) == tick_interval_time;

            self.tick_interval(should_signal_proposal).await?;
        }
        Ok(())
    }

    pub async fn get_latest_justified(&self) -> anyhow::Result<Option<Checkpoint>> {
        let mut latest_justified: Option<Checkpoint> = None;
        let state_provider = self.store.lock().await.lean_state_provider();
        let state_iter = state_provider.iter_values()?;
        for state in state_iter {
            let state = state?;
            match &latest_justified {
                Some(current)
                    if current.slot > state.latest_justified.slot
                        || state.latest_justified.root == B256::ZERO => {}
                _ => {
                    latest_justified = Some(state.latest_justified);
                }
            }
        }
        Ok(latest_justified)
    }

    /// Done upon processing new attestations or a new block
    pub async fn update_head(&self) -> anyhow::Result<()> {
        let (
            latest_known_attestations,
            latest_justified_provider,
            states,
            head_provider,
            latest_finalized_provider,
            block_provider,
        ) = {
            let db = self.store.lock().await;
            (
                db.latest_known_attestations_provider()
                    .get_all_attestations()?,
                db.latest_justified_provider(),
                db.lean_state_provider(),
                db.lean_head_provider(),
                db.latest_finalized_provider(),
                db.lean_block_provider(),
            )
        };
        let latest_justified = match self.get_latest_justified().await? {
            Some(latest_justified) => latest_justified,
            None => latest_justified_provider.get()?,
        };

        let new_head = self
            .get_fork_choice_head(
                latest_known_attestations.into_values().map(Ok),
                latest_justified.root,
                0,
            )
            .await?;

        let latest_finalized = match states.get(new_head)? {
            Some(state) => state.latest_finalized,
            None => latest_justified_provider.get()?,
        };

        set_int_gauge_vec(
            &HEAD_SLOT,
            block_provider
                .get(new_head)?
                .ok_or(anyhow!("Failed to get head slot"))?
                .message
                .block
                .slot as i64,
            &[],
        );

        head_provider.insert(new_head)?;
        latest_justified_provider.insert(latest_justified)?;
        latest_finalized_provider.insert(latest_finalized)?;

        Ok(())
    }

    async fn get_attestation_target(&self) -> anyhow::Result<Checkpoint> {
        let (head_provider, block_provider, safe_target_provider, latest_finalized_provider) = {
            let db = self.store.lock().await;
            (
                db.lean_head_provider(),
                db.lean_block_provider(),
                db.lean_safe_target_provider(),
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
    ///
    /// See lean specification:
    /// <https://github.com/leanEthereum/leanSpec/blob/4b750f2748a3718fe3e1e9cdb3c65e3a7ddabff5/src/lean_spec/subspecs/forkchoice/store.py#L319-L339>
    pub async fn get_proposal_head(&self, slot: u64) -> anyhow::Result<B256> {
        let slot_time =
            lean_network_spec().genesis_time + slot * lean_network_spec().seconds_per_slot;
        self.on_tick(slot_time, true).await?;
        self.accept_new_attestations().await?;
        Ok(self.store.lock().await.lean_head_provider().get()?)
    }

    pub async fn produce_block_with_signatures(
        &self,
        slot: u64,
        validator_index: u64,
    ) -> anyhow::Result<(Block, Vec<FixedBytes<4000>>)> {
        let head_root = self.get_proposal_head(slot).await?;
        let initialize_block_timer = start_timer_vec(&PROPOSE_BLOCK_TIME, &["initialize_block"]);
        let (state_provider, latest_known_attestation_provider, block_provider) = {
            let db = self.store.lock().await;
            (
                db.lean_state_provider(),
                db.latest_known_attestations_provider(),
                db.lean_block_provider(),
            )
        };
        let mut head_state = state_provider
            .get(head_root)?
            .ok_or(anyhow!("State not found for head root"))?;
        stop_timer(initialize_block_timer);

        let num_validators = head_state.validators.len();

        ensure!(
            is_proposer(validator_index, slot, num_validators as u64),
            "Validator {validator_index} is not the proposer for slot {slot}"
        );

        let add_attestations_timer =
            start_timer_vec(&PROPOSE_BLOCK_TIME, &["add_valid_attestations_to_block"]);

        let mut attestations = VariableList::empty();
        let mut signatures: Vec<FixedBytes<4000>> = Vec::new();

        loop {
            let candidate_block = Block {
                slot,
                proposer_index: validator_index,
                parent_root: head_root,
                state_root: B256::ZERO,
                body: BlockBody {
                    attestations: attestations.clone(),
                },
            };
            let mut advanced_state = head_state.clone();
            advanced_state.process_slots(slot)?;
            advanced_state.process_block(&candidate_block)?;

            let mut new_attestations: VariableList<Attestation, U4096> = VariableList::empty();
            let mut new_signatures: Vec<FixedBytes<4000>> = Vec::new();

            for signed_attestation in latest_known_attestation_provider
                .get_all_attestations()?
                .values()
            {
                let data = &signed_attestation.message.data;
                if !block_provider.contains_key(data.head.root) {
                    continue;
                }
                if data.source != advanced_state.latest_justified {
                    continue;
                }
                if !attestations.contains(&signed_attestation.message) {
                    new_attestations
                        .push(signed_attestation.message.clone())
                        .map_err(|err| anyhow!("Could not append attestation: {err:?}"))?;
                    new_signatures.push(signed_attestation.signature);
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

            for signature in new_signatures {
                signatures.push(signature);
            }
        }
        stop_timer(add_attestations_timer);
        head_state.process_slots(slot)?;

        let mut final_block = Block {
            slot,
            proposer_index: validator_index,
            parent_root: head_root,
            state_root: B256::ZERO,
            body: BlockBody { attestations },
        };
        head_state.process_block(&final_block)?;
        let compute_state_root_timer =
            start_timer_vec(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        final_block.state_root = head_state.tree_hash_root();
        stop_timer(compute_state_root_timer);
        Ok((final_block, signatures))
    }

    pub async fn on_block(
        &mut self,
        signed_block_with_attestation: &SignedBlockWithAttestation,
    ) -> anyhow::Result<()> {
        let (state_provider, block_provider) = {
            let db = self.store.lock().await;
            (db.lean_state_provider(), db.lean_block_provider())
        };
        let block = &signed_block_with_attestation.message.block;
        let proposer_attestation = &signed_block_with_attestation.message.proposer_attestation;
        let block_root = block.tree_hash_root();

        if block_provider.get(block_root)?.is_some() {
            return Ok(());
        }

        let mut parent_state = state_provider
            .get(block.parent_root)?
            .ok_or(anyhow!("State not found for parent root"))?;

        // TODO: Add signature validation, https://github.com/ReamLabs/ream/issues/848.
        parent_state.state_transition(block, true)?;

        block_provider.insert(block_root, signed_block_with_attestation.clone())?;
        state_provider.insert(block_root, parent_state)?;

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

        self.update_head().await?;

        self.on_attestation(
            SignedAttestation {
                message: proposer_attestation.clone(),
                // TODO: Add signature, https://github.com/ReamLabs/ream/issues/848.
                signature: FixedBytes::<4000>::default(),
            },
            false,
        )
        .await?;

        Ok(())
    }

    pub async fn validate_attestation(
        &self,
        signed_attestation: &SignedAttestation,
    ) -> anyhow::Result<()> {
        let attestation = &signed_attestation.message;
        let data = &attestation.data;
        let block_provider = self.store.lock().await.lean_block_provider();

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

        let source_block = block_provider
            .get(data.source.root)?
            .ok_or(anyhow!("Failed to get source block"))?;
        let target_block = block_provider
            .get(data.target.root)?
            .ok_or(anyhow!("Failed to get target block"))?;

        ensure!(
            source_block.message.block.slot <= target_block.message.block.slot,
            "Source slot must not exceed target"
        );
        ensure!(
            data.source.slot <= data.target.slot,
            "Source checkpoint slot must not exceed target"
        );

        ensure!(
            source_block.message.block.slot == data.source.slot,
            "Source checkpoint slot mismatch"
        );
        ensure!(
            target_block.message.block.slot == data.target.slot,
            "Target checkpoint slot mismatch"
        );

        let current_slot = self.store.lock().await.lean_time_provider().get()?
            / lean_network_spec().seconds_per_slot;
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
                db.lean_latest_new_attestations_provider(),
                db.lean_time_provider(),
            )
        };
        self.validate_attestation(&signed_attestation).await?;
        let validator_id = signed_attestation.message.validator_id;
        let attestation_slot = signed_attestation.message.data.slot;
        if is_from_block {
            let latest_known = match latest_known_attestations_provider.get(validator_id)? {
                Some(latest_known) => latest_known.message.data.slot < attestation_slot,
                None => true,
            };
            if latest_known {
                latest_known_attestations_provider.insert(validator_id, signed_attestation)?;
            }
            let remove = match latest_new_attestations_provider.get(validator_id)? {
                Some(new_new) => new_new.message.data.slot <= attestation_slot,
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
                Some(latest_new) => latest_new.message.data.slot < attestation_slot,
                None => true,
            };
            if latest_new {
                latest_new_attestations_provider.insert(validator_id, signed_attestation)?;
            }
        }

        Ok(())
    }

    pub async fn produce_attestation(&self, slot: u64) -> anyhow::Result<AttestationData> {
        let (head_provider, block_provider, latest_justified_provider) = {
            let db = self.store.lock().await;
            (
                db.lean_head_provider(),
                db.lean_block_provider(),
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
