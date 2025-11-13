use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use ream_consensus_lean::{
    attestation::{Attestation, AttestationData, SignedAttestation},
    block::{Block, BlockBody, BlockWithSignatures, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
    state::LeanState,
    validator::is_proposer,
};
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
use ream_metrics::{
    ATTESTATION_VALIDATION_TIME, ATTESTATIONS_INVALID_TOTAL, ATTESTATIONS_VALID_TOTAL,
    FINALIZED_SLOT, FORK_CHOICE_BLOCK_PROCESSING_TIME, HEAD_SLOT, JUSTIFIED_SLOT,
    LATEST_FINALIZED_SLOT, LATEST_JUSTIFIED_SLOT, PROPOSE_BLOCK_TIME, VALIDATORS_COUNT,
    inc_int_counter_vec, set_int_gauge_vec, start_timer, stop_timer,
};
use ream_network_spec::networks::lean_network_spec;
use ream_network_state_lean::NetworkState;
use ream_post_quantum_crypto::leansig::signature::Signature;
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

        set_int_gauge_vec(
            &VALIDATORS_COUNT,
            lean_network_spec().num_validators as i64,
            &[],
        );

        Ok(Store {
            store: Arc::new(Mutex::new(db)),
            network_state: Arc::new(NetworkState::new(anchor_checkpoint, anchor_checkpoint)),
        })
    }

    /// Use LMD GHOST to get the head, given a particular root (usually the
    /// latest known justified block)
    async fn compute_lmd_ghost_head(
        &self,
        attestations: impl Iterator<Item = anyhow::Result<SignedAttestation>>,
        provided_root: B256,
        min_score: u64,
    ) -> anyhow::Result<B256> {
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

        let start_slot = block_provider.get(root)?.expect("msg").message.block.slot;
        // For each block, count the number of votes for that block. A vote
        // for any descendant of a block also counts as a vote for that block
        let mut weights = HashMap::<B256, u64>::new();

        for attestation in attestations {
            let attestation = attestation?;
            let mut current_root = attestation.message.data.head.root;

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

        while let Some(children) = children_map.get(&head) {
            head = *children
                .iter()
                .max_by_key(|child_hash| {
                    let vote_weight = weights.get(*child_hash).unwrap_or(&0);
                    let slot = block_provider
                        .get(**child_hash)
                        .map(|maybe_block| match maybe_block {
                            Some(block) => block.message.block.slot,
                            None => 0,
                        })
                        .unwrap_or(0);
                    (*vote_weight, slot, *(*child_hash))
                })
                .ok_or_else(|| anyhow!("No children found for current root: {head}"))?;
        }

        Ok(head)
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

        safe_target_provider.insert(
            self.compute_lmd_ghost_head(
                latest_new_attestations_provider.iter_values()?,
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
        let (latest_known_attestations, latest_justified_provider, head_provider, block_provider) = {
            let db = self.store.lock().await;
            (
                db.latest_known_attestations_provider()
                    .get_all_attestations()?,
                db.latest_justified_provider(),
                db.head_provider(),
                db.block_provider(),
            )
        };

        let new_head = self
            .compute_lmd_ghost_head(
                latest_known_attestations.into_values().map(Ok),
                latest_justified_provider.get()?.root,
                0,
            )
            .await?;

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
        let head_block = block_provider
            .get(new_head)?
            .ok_or(anyhow!("Failed to get head block"))?;
        *self.network_state.head_checkpoint.write() = Checkpoint {
            root: head_block.message.block.tree_hash_root(),
            slot: head_block.message.block.slot,
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

    pub async fn produce_block_with_signatures(
        &self,
        slot: u64,
        validator_index: u64,
    ) -> anyhow::Result<BlockWithSignatures> {
        let head_root = self.get_proposal_head(slot).await?;
        let initialize_block_timer = start_timer(&PROPOSE_BLOCK_TIME, &["initialize_block"]);
        let (state_provider, latest_known_attestation_provider, block_provider) = {
            let db = self.store.lock().await;
            (
                db.state_provider(),
                db.latest_known_attestations_provider(),
                db.block_provider(),
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
            start_timer(&PROPOSE_BLOCK_TIME, &["add_valid_attestations_to_block"]);

        let mut attestations = VariableList::empty();
        let mut signatures: Vec<Signature> = Vec::new();

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
            let mut new_signatures: Vec<Signature> = Vec::new();
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
        let compute_state_root_timer = start_timer(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        final_block.state_root = head_state.tree_hash_root();
        stop_timer(compute_state_root_timer);
        Ok(BlockWithSignatures {
            block: final_block,
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
                signature: *signatures
                    .get(block.body.attestations.len())
                    .ok_or(anyhow!("Failed to get attestation"))?,
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
        let data = &signed_attestation.message.data;
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
    use alloy_primitives::B256;
    use ream_consensus_lean::{
        attestation::{Attestation, AttestationData, SignedAttestation},
        block::{Block, BlockWithAttestation, BlockWithSignatures, SignedBlockWithAttestation},
        checkpoint::Checkpoint,
        state::LeanState,
        utils::generate_default_validators,
    };
    use ream_network_spec::networks::{LeanNetworkSpec, set_lean_network_spec};
    use ream_post_quantum_crypto::leansig::signature::Signature;
    use ream_storage::{
        db::{ReamDB, lean::LeanDB},
        tables::{field::REDBField, table::REDBTable},
    };
    use ssz_types::{VariableList, typenum::U4096};
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
        let (genesis_block, genesis_state) =
            setup_genesis(0, generate_default_validators(no_of_validators));

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

        set_lean_network_spec(LeanNetworkSpec::ephemery().into());

        (
            Store::get_forkchoice_store(
                signed_genesis_block,
                genesis_state.clone(),
                db_setup(),
                None,
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
                proposer_attestation: Attestation {
                    validator_id: block.proposer_index,
                    data: attestation_data,
                },
                block,
            },
            signature: signatures,
        }
    }

    // BLOCK PRODUCTION TESTS

    /// Test basic block production by authorized proposer.
    #[tokio::test]
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
            signature: Signature::blank(),
        };

        let attestation_2 = SignedAttestation {
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
            signature: Signature::blank(),
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

        let attestation = Attestation {
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

        let attestation = Attestation {
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
        let attestation = Attestation {
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
            let attestation = Attestation {
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
        let validator_id = 3;

        let (store, _) = sample_store(10).await;
        let latest_justified_provider = store.store.lock().await.latest_justified_provider();

        let attestation_1 = Attestation {
            validator_id,
            data: store.produce_attestation_data(1).await.unwrap(),
        };

        let attestation_2 = Attestation {
            validator_id,
            data: store.produce_attestation_data(2).await.unwrap(),
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

        let attestation = Attestation {
            validator_id: 2,
            data: store.produce_attestation_data(5).await.unwrap(),
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
}
