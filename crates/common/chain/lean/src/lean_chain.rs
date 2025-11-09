use std::{collections::HashMap, sync::Arc};

use alloy_primitives::{B256, FixedBytes};
use anyhow::anyhow;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAttestation},
    block::{Block, BlockBody, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
    is_justifiable_slot,
    state::LeanState,
};
use ream_fork_choice::lean::get_fork_choice_head;
use ream_metrics::{HEAD_SLOT, PROPOSE_BLOCK_TIME, set_int_gauge_vec, start_timer_vec, stop_timer};
use ream_network_spec::networks::lean_network_spec;
use ream_storage::{
    db::lean::LeanDB,
    tables::{field::REDBField, lean::lean_block::LeanBlockTable, table::REDBTable},
};
use ream_sync::rwlock::{Reader, Writer};
use tokio::sync::Mutex;
use tree_hash::TreeHash;

pub type LeanChainWriter = Writer<LeanChain>;
pub type LeanChainReader = Reader<LeanChain>;

/// [LeanChain] represents the state that the Lean node should maintain.
///
/// Most of the fields are based on the Python implementation of [`Staker`](https://github.com/ethereum/research/blob/d225a6775a9b184b5c1fd6c830cc58a375d9535f/3sf-mini/p2p.py#L15-L42),
/// but doesn't include `validator_id` as a node should manage multiple validators.
#[derive(Debug, Clone)]
pub struct LeanChain {
    /// Database.
    pub store: Arc<Mutex<LeanDB>>,
    /// Attestations that we have received but not yet taken into account.
    /// Maps validator id to signed attestation.
    pub latest_new_attestations: HashMap<u64, SignedAttestation>,
}

impl LeanChain {
    pub fn new(
        genesis_block: SignedBlockWithAttestation,
        genesis_state: LeanState,
        db: LeanDB,
    ) -> LeanChain {
        let genesis_block_hash = genesis_block.message.block.tree_hash_root();
        db.lean_block_provider()
            .insert(genesis_block_hash, genesis_block)
            .expect("Failed to insert genesis block");
        db.latest_finalized_provider()
            .insert(genesis_state.latest_finalized)
            .expect("Failed to insert latest finalized checkpoint");
        db.latest_justified_provider()
            .insert(genesis_state.latest_justified)
            .expect("Failed to insert latest justified checkpoint");
        db.lean_state_provider()
            .insert(genesis_block_hash, genesis_state)
            .expect("Failed to insert genesis state");

        LeanChain {
            store: Arc::new(Mutex::new(db)),
            latest_new_attestations: HashMap::new(),
        }
    }

    pub async fn get_block_id_by_slot(&self, slot: u64) -> anyhow::Result<B256> {
        self.store
            .lock()
            .await
            .slot_index_provider()
            .get(slot)?
            .ok_or_else(|| anyhow!("Block not found in chain for slot: {slot}"))
    }

    pub async fn get_block_by_slot(&self, slot: u64) -> anyhow::Result<SignedBlockWithAttestation> {
        let (lean_block_provider, lean_slot_provider) = {
            let db = self.store.lock().await;
            (db.lean_block_provider(), db.slot_index_provider())
        };

        let block_hash = lean_slot_provider
            .get(slot)?
            .ok_or_else(|| anyhow!("Block hash not found in chain for slot: {slot}"))?;

        lean_block_provider
            .get(block_hash)?
            .ok_or_else(|| anyhow!("Block not found in chain for slot: {slot}"))
    }

    /// Compute the latest block that the validator is allowed to choose as the target
    /// and update as a safe target.
    ///
    /// See lean specification:
    /// <https://github.com/leanEthereum/leanSpec/blob/f8e8d271d8b8b6513d34c78692aff47438d6fa18/src/lean_spec/subspecs/forkchoice/store.py#L301-L317>
    pub async fn update_safe_target(&mut self) -> anyhow::Result<()> {
        // 2/3rd majority min voting weight for target selection
        // Note that we use ceiling division here.
        let min_target_score = (lean_network_spec().num_validators * 2).div_ceil(3);
        let latest_justified_root = self
            .store
            .lock()
            .await
            .latest_justified_provider()
            .get()?
            .root;

        self.store.lock().await.lean_safe_target_provider().insert(
            get_fork_choice_head(
                self.store.clone(),
                &self.latest_new_attestations,
                &latest_justified_root,
                min_target_score,
            )
            .await?,
        )?;

        Ok(())
    }

    /// Process new attestations that the staker has received. Attestation processing is done
    /// at a particular time, because of safe target and view merge rule
    pub async fn accept_new_attestations(&mut self) -> anyhow::Result<()> {
        let latest_known_attestation_provider = {
            let db = self.store.lock().await;
            db.latest_known_attestations_provider()
        };

        latest_known_attestation_provider.batch_insert(self.latest_new_attestations.drain())?;

        self.update_head().await?;
        Ok(())
    }

    /// Done upon processing new attestations or a new block
    pub async fn update_head(&mut self) -> anyhow::Result<()> {
        let (latest_known_attestations, latest_justified_root, latest_finalized_checkpoint) = {
            let db = self.store.lock().await;
            let head = db.lean_head_provider().get()?;
            (
                db.latest_known_attestations_provider()
                    .get_all_attestations()?,
                db.latest_justified_provider().get()?.root,
                db.lean_state_provider()
                    .get(head)?
                    .ok_or_else(|| anyhow!("State not found in chain for head: {head}"))?
                    .latest_finalized,
            )
        };

        // Update head.
        let head = get_fork_choice_head(
            self.store.clone(),
            &latest_known_attestations,
            &latest_justified_root,
            0,
        )
        .await?;
        self.store.lock().await.lean_head_provider().insert(head)?;

        // Send latest head slot to metrics
        let head_slot = self
            .store
            .lock()
            .await
            .lean_block_provider()
            .get(head)?
            .ok_or_else(|| anyhow!("Block not found for head: {head}"))?
            .message
            .block
            .slot;

        set_int_gauge_vec(&HEAD_SLOT, head_slot as i64, &[]);

        // Update latest finalized checkpoint in DB.
        self.store
            .lock()
            .await
            .latest_finalized_provider()
            .insert(latest_finalized_checkpoint)?;

        Ok(())
    }

    /// Calculate target checkpoint for validator attestations.
    /// Determines appropriate attestation target based on head, safe target,
    /// and finalization constraints.
    ///
    /// See lean specification:
    /// <https://github.com/leanEthereum/leanSpec/blob/f8e8d271d8b8b6513d34c78692aff47438d6fa18/src/lean_spec/subspecs/forkchoice/store.py#L341-L366>
    pub async fn get_attestation_target(
        &self,
        lean_block_provider: &LeanBlockTable,
        finalized_slot: u64,
    ) -> anyhow::Result<Checkpoint> {
        // Start from current head
        let head = self.store.lock().await.lean_head_provider().get()?;
        let mut target_block = lean_block_provider
            .get(head)?
            .ok_or_else(|| anyhow!("Block not found in chain for head: {head}"))?
            .message
            .block;

        // Walk back up to 3 steps if safe target is newer
        for _ in 0..3 {
            let safe_target = self.store.lock().await.lean_safe_target_provider().get()?;
            let safe_target_block = lean_block_provider
                .get(safe_target)?
                .ok_or_else(|| anyhow!("Block not found for safe target hash: {safe_target}"))?
                .message
                .block;
            if target_block.slot > safe_target_block.slot {
                target_block = lean_block_provider
                    .get(target_block.parent_root)?
                    .ok_or_else(|| {
                        anyhow!(
                            "Block not found for target block's parent hash: {}",
                            target_block.parent_root
                        )
                    })?
                    .message
                    .block;
            }
        }

        // Ensure target is in justifiable slot range
        while !is_justifiable_slot(finalized_slot, target_block.slot) {
            target_block = lean_block_provider
                .get(target_block.parent_root)?
                .ok_or_else(|| {
                    anyhow!(
                        "Block not found for target block's parent hash: {}",
                        target_block.parent_root
                    )
                })?
                .message
                .block;
        }

        Ok(Checkpoint {
            root: target_block.tree_hash_root(),
            slot: target_block.slot,
        })
    }

    /// Get the head for block proposal at given slot.
    /// Ensures store is up-to-date and processes any pending attestations.
    ///
    /// See lean specification:
    /// <https://github.com/leanEthereum/leanSpec/blob/4b750f2748a3718fe3e1e9cdb3c65e3a7ddabff5/src/lean_spec/subspecs/forkchoice/store.py#L319-L339>
    pub async fn get_proposal_head(&mut self) -> anyhow::Result<B256> {
        self.accept_new_attestations().await?;
        Ok(self.store.lock().await.lean_head_provider().get()?)
    }

    pub async fn propose_block(
        &mut self,
        slot: u64,
    ) -> anyhow::Result<(Block, Vec<FixedBytes<4000>>)> {
        let head = self.get_proposal_head().await?;

        let initialize_block_timer = start_timer_vec(&PROPOSE_BLOCK_TIME, &["initialize_block"]);

        let (lean_state_provider, latest_known_attestation_provider) = {
            let db = self.store.lock().await;
            (
                db.lean_state_provider(),
                db.latest_known_attestations_provider(),
            )
        };

        let head_state = lean_state_provider
            .get(head)?
            .ok_or_else(|| anyhow!("Post state not found for head: {head}"))?;

        let mut new_block = Block {
            slot,
            proposer_index: slot % lean_network_spec().num_validators,
            parent_root: head,
            state_root: B256::ZERO,
            body: BlockBody::default(),
        };
        stop_timer(initialize_block_timer);

        // Clone state so we can apply the new block to get a new state
        let mut state = head_state.clone();
        let mut signatures = vec![];

        // Apply state transition so the state is brought up to the expected slot
        state.state_transition(&new_block, true)?;

        // Keep attempt to add valid attestations from the list of available attestations
        let add_attestations_timer =
            start_timer_vec(&PROPOSE_BLOCK_TIME, &["add_valid_attestations_to_block"]);
        loop {
            state.process_attestations(&new_block.body.attestations)?;
            let mut new_attestations_to_add = Vec::new();
            let mut new_signatures_to_add = Vec::new();

            for signed_attestation in latest_known_attestation_provider
                .get_all_attestations()?
                .values()
            {
                if signed_attestation.message.source() == state.latest_justified
                    && !new_block
                        .body
                        .attestations
                        .contains(&signed_attestation.message)
                {
                    new_attestations_to_add.push(signed_attestation.message.clone());
                    new_signatures_to_add.push(signed_attestation.signature);
                }
            }

            if new_attestations_to_add.is_empty() {
                break;
            }

            for attestation in new_attestations_to_add {
                new_block
                    .body
                    .attestations
                    .push(attestation)
                    .map_err(|err| anyhow!("Failed to add attestation to new_block: {err:?}"))?;
            }
            for signature in new_signatures_to_add {
                signatures.push(signature);
            }
        }
        stop_timer(add_attestations_timer);

        // Update `state.latest_block_header.body_root` so that it accounts for
        // the attestations that we've added above
        state.latest_block_header.body_root = new_block.body.tree_hash_root();

        // Compute the state root
        let compute_state_root_timer =
            start_timer_vec(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        new_block.state_root = state.tree_hash_root();
        stop_timer(compute_state_root_timer);

        Ok((new_block, signatures))
    }

    pub async fn build_attestation_data(&self, slot: u64) -> anyhow::Result<AttestationData> {
        let (head, target, source) = {
            let db = self.store.lock().await;
            let head = db.lean_head_provider().get()?;
            (
                Checkpoint {
                    root: head,
                    slot: db
                        .lean_block_provider()
                        .get(head)?
                        .ok_or_else(|| anyhow!("Block not found for head: {head}"))?
                        .message
                        .block
                        .slot,
                },
                self.get_attestation_target(
                    &db.lean_block_provider(),
                    db.latest_finalized_provider().get()?.slot,
                )
                .await?,
                db.latest_justified_provider().get()?,
            )
        };
        Ok(AttestationData {
            slot,
            head,
            target,
            source,
        })
    }

    /// Processes a new block, updates the store, and triggers a head update.
    ///
    /// See lean specification:
    /// <https://github.com/leanEthereum/leanSpec/blob/ee16b19825a1f358b00a6fc2d7847be549daa03b/docs/client/forkchoice.md?plain=1#L314-L342>
    pub async fn on_block(
        &mut self,
        signed_block_with_attestation: SignedBlockWithAttestation,
    ) -> anyhow::Result<()> {
        let block = &signed_block_with_attestation.message.block;
        let block_hash = signed_block_with_attestation.message.block.tree_hash_root();

        let (lean_block_provider, latest_justified_provider, lean_state_provider) = {
            let db = self.store.lock().await;
            (
                db.lean_block_provider(),
                db.latest_justified_provider(),
                db.lean_state_provider(),
            )
        };

        // If the block is already known, ignore it
        if lean_block_provider.contains_key(block_hash) {
            return Ok(());
        }

        let mut state = lean_state_provider.get(block.parent_root)?.ok_or_else(|| {
            anyhow!(
                "Parent state not found for block: {block_hash}, parent: {}",
                block.parent_root
            )
        })?;

        // TODO: Add signature validation once spec is complete.
        // Tracking issue: https://github.com/ReamLabs/ream/issues/881
        state.state_transition(block, true)?;

        let mut signed_attestations = vec![];
        for attestation in &block.body.attestations {
            signed_attestations.push(SignedAttestation {
                message: attestation.clone(),
                signature: FixedBytes::<4000>::default(),
            });
        }
        lean_block_provider.insert(block_hash, signed_block_with_attestation)?;
        latest_justified_provider.insert(state.latest_justified)?;
        lean_state_provider.insert(block_hash, state)?;
        self.on_attestation_from_block(signed_attestations).await?;
        self.update_head().await?;

        Ok(())
    }

    /// Process multiple attestations (multiple [SignedAttestation]s) from [SignedBlock].
    /// Main reason to have this function is to avoid multiple DB transactions by
    /// batch inserting attestations.
    ///
    /// See lean specification:
    /// <https://github.com/leanEthereum/leanSpec/blob/ee16b19825a1f358b00a6fc2d7847be549daa03b/docs/client/forkchoice.md?plain=1#L279-L312>
    pub async fn on_attestation_from_block(
        &mut self,
        signed_attestations: impl IntoIterator<Item = SignedAttestation>,
    ) -> anyhow::Result<()> {
        let latest_known_attestation_provider = {
            let db = self.store.lock().await;
            db.latest_known_attestations_provider()
        };

        latest_known_attestation_provider.batch_insert(
            signed_attestations
                .into_iter()
                .filter_map(|signed_attestation| {
                    let validator_id = signed_attestation.message.validator_id;

                    // Clear from new attestations if this is latest.
                    if let Some(latest_attestation) =
                        self.latest_new_attestations.get(&validator_id)
                        && latest_attestation.message.slot() < signed_attestation.message.slot()
                    {
                        self.latest_new_attestations.remove(&validator_id);
                    }

                    // Filter for batch insertion.
                    latest_known_attestation_provider
                        .get(validator_id)
                        .ok()
                        .flatten()
                        .is_none_or(|latest_attestation| {
                            latest_attestation.message.slot() < signed_attestation.message.slot()
                        })
                        .then_some((validator_id, signed_attestation))
                }),
        )?;

        Ok(())
    }

    /// Processes a single attestation ([SignedAttestation]) from gossip.
    ///
    /// See lean specification:
    /// <https://github.com/leanEthereum/leanSpec/blob/ee16b19825a1f358b00a6fc2d7847be549daa03b/docs/client/forkchoice.md?plain=1#L279-L312>
    pub fn on_attestation_from_gossip(&mut self, signed_attestation: SignedAttestation) {
        let validator_id = signed_attestation.message.validator_id;

        // Update latest new attestations if this is the latest
        if self
            .latest_new_attestations
            .get(&validator_id)
            .is_none_or(|latest_attestation| {
                latest_attestation.message.slot() < signed_attestation.message.slot()
            })
        {
            self.latest_new_attestations
                .insert(validator_id, signed_attestation.clone());
        }
    }
}
