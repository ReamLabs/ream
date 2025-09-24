use std::sync::Arc;

use alloy_primitives::{B256, FixedBytes};
use anyhow::anyhow;
use ream_consensus_lean::{
    block::{Block, BlockBody, SignedBlock},
    checkpoint::Checkpoint,
    is_justifiable_slot,
    state::LeanState,
    vote::{SignedVote, Vote},
};
use ream_fork_choice::lean::get_fork_choice_head;
use ream_metrics::{PROPOSE_BLOCK_TIME, start_timer_vec, stop_timer};
use ream_network_spec::networks::lean_network_spec;
use ream_storage::{db::lean::LeanDB, tables::table::Table};
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
    /// Votes that we have received but not yet taken into account.
    pub new_votes: Vec<SignedVote>,
    /// Initialize the chain with the genesis block.
    pub genesis_hash: B256,
    /// Number of validators.
    pub num_validators: u64,
    /// Block that it is safe to use to vote as the target.
    /// Diverge from Python implementation: Use genesis hash instead of `None`.
    pub safe_target: B256,
    /// Head of the chain.
    pub head: B256,
}

impl LeanChain {
    pub fn new(genesis_block: SignedBlock, genesis_state: LeanState, db: LeanDB) -> LeanChain {
        let genesis_block_hash = genesis_block.message.tree_hash_root();
        let no_of_validators = genesis_state.config.num_validators;
        db.lean_block_provider()
            .insert(genesis_block_hash, genesis_block)
            .expect("Failed to insert genesis block");
        db.lean_state_provider()
            .insert(genesis_block_hash, genesis_state)
            .expect("Failed to insert genesis state");

        LeanChain {
            store: Arc::new(Mutex::new(db)),
            new_votes: Vec::new(),
            genesis_hash: genesis_block_hash,
            num_validators: no_of_validators,
            safe_target: genesis_block_hash,
            head: genesis_block_hash,
        }
    }

    pub async fn latest_justified_hash(&self) -> anyhow::Result<B256> {
        self.store
            .lock()
            .await
            .lean_state_provider()
            .get(self.head)?
            .ok_or_else(|| anyhow!("State not found in chain for head: {}", self.head))
            .map(|state| state.latest_justified.root)
    }

    pub async fn get_block_id_by_slot(&self, slot: u64) -> anyhow::Result<B256> {
        self.store
            .lock()
            .await
            .slot_index_provider()
            .get(slot)?
            .ok_or_else(|| anyhow!("Block not found in chain for head: {}", self.head))
    }

    pub async fn get_block_by_slot(&self, slot: u64) -> anyhow::Result<SignedBlock> {
        let (lean_block_provider, lean_slot_provider) = {
            let db = self.store.lock().await;
            (db.lean_block_provider(), db.slot_index_provider())
        };

        let block_hash = lean_slot_provider
            .get(slot)?
            .ok_or_else(|| anyhow!("Block hash not found in chain for head: {}", self.head))?;

        lean_block_provider
            .get(block_hash)?
            .ok_or_else(|| anyhow!("Block not found in chain for head: {}", self.head))
    }

    pub async fn latest_finalized_hash(&self) -> anyhow::Result<B256> {
        self.store
            .lock()
            .await
            .lean_state_provider()
            .get(self.head)?
            .ok_or_else(|| anyhow!("State not found in chain for head: {}", self.head))
            .map(|state| state.latest_finalized.root)
    }

    /// Compute the latest block that the staker is allowed to choose as the target
    pub async fn compute_safe_target(&self) -> anyhow::Result<B256> {
        let justified_hash = self.latest_justified_hash().await?;

        get_fork_choice_head(
            self.store.clone(),
            &self.new_votes,
            &justified_hash,
            self.num_validators * 2 / 3,
        )
        .await
    }

    /// Process new votes that the staker has received. Vote processing is done
    /// at a particular time, because of safe target and view merge rule
    pub async fn accept_new_votes(&mut self) -> anyhow::Result<()> {
        let known_votes_provider = {
            let db = self.store.lock().await;
            db.known_votes_provider()
        };

        let mut votes_to_be_inserted = Vec::new();
        for new_vote in self.new_votes.drain(..) {
            if !known_votes_provider.contains(&new_vote)? {
                votes_to_be_inserted.push(new_vote);
            }
        }

        known_votes_provider.batch_append(votes_to_be_inserted)?;

        self.recompute_head().await?;
        Ok(())
    }

    /// Done upon processing new votes or a new block
    pub async fn recompute_head(&mut self) -> anyhow::Result<()> {
        let justified_hash = self.latest_justified_hash().await?;
        let known_votes_provider = {
            let db = self.store.lock().await;
            db.known_votes_provider()
        };

        let votes = known_votes_provider.get_all_votes()?;
        self.head = get_fork_choice_head(self.store.clone(), &votes, &justified_hash, 0).await?;

        Ok(())
    }

    pub async fn propose_block(&self, slot: u64) -> anyhow::Result<Block> {
        let initialize_block_timer = start_timer_vec(&PROPOSE_BLOCK_TIME, &["initialize_block"]);

        let (lean_state_provider, known_votes_provider) = {
            let db = self.store.lock().await;
            (db.lean_state_provider(), db.known_votes_provider())
        };

        let head_state = lean_state_provider
            .get(self.head)?
            .ok_or_else(|| anyhow!("Post state not found for head: {}", self.head))?;

        let mut new_block = SignedBlock {
            message: Block {
                slot,
                proposer_index: slot % lean_network_spec().num_validators,
                parent_root: self.head,
                // Diverged from Python implementation: Using `B256::ZERO` instead of `None`)
                state_root: B256::ZERO,
                body: BlockBody::default(),
            },
            signature: FixedBytes::default(),
        };
        stop_timer(initialize_block_timer);

        // Clone state so we can apply the new block to get a new state
        let mut state = head_state.clone();

        // Apply state transition so the state is brought up to the expected slot
        state.state_transition(&new_block, true, false)?;

        // Keep attempt to add valid votes from the list of available votes
        let add_votes_timer = start_timer_vec(&PROPOSE_BLOCK_TIME, &["add_valid_votes_to_block"]);
        loop {
            state.process_attestations(&new_block.message.body.attestations)?;
            let new_votes_to_add = known_votes_provider
                .filter_new_votes_to_add(state.latest_justified.root, &new_block)?;

            if new_votes_to_add.is_empty() {
                break;
            }

            for vote in new_votes_to_add {
                new_block
                    .message
                    .body
                    .attestations
                    .push(vote)
                    .map_err(|err| anyhow!("Failed to add vote to new_block: {err:?}"))?;
            }
        }
        stop_timer(add_votes_timer);

        // Update `state.latest_block_header.body_root` so that it accounts for
        // the votes that we've added above
        state.latest_block_header.body_root = new_block.message.body.tree_hash_root();

        // Compute the state root
        let compute_state_root_timer =
            start_timer_vec(&PROPOSE_BLOCK_TIME, &["compute_state_root"]);
        new_block.message.state_root = state.tree_hash_root();
        stop_timer(compute_state_root_timer);

        Ok(new_block.message)
    }

    pub async fn build_vote(&self, slot: u64) -> anyhow::Result<Vote> {
        let (lean_block_provider, lean_state_provider) = {
            let db = self.store.lock().await;
            (db.lean_block_provider(), db.lean_state_provider())
        };

        let state = lean_state_provider
            .get(self.head)?
            .ok_or_else(|| anyhow!("Post state not found for head: {}", self.head))?;

        let mut target_block = lean_block_provider
            .get(self.head)?
            .ok_or_else(|| anyhow!("Block not found in chain for head: {}", self.head))?;

        // If there is no very recent safe target, then vote for the k'th ancestor
        // of the head
        for _ in 0..3 {
            let safe_target_block =
                lean_block_provider.get(self.safe_target)?.ok_or_else(|| {
                    anyhow!("Block not found for safe target hash: {}", self.safe_target)
                })?;
            if target_block.message.slot > safe_target_block.message.slot {
                target_block = lean_block_provider
                    .get(target_block.message.parent_root)?
                    .ok_or_else(|| {
                        anyhow!(
                            "Block not found for target block's parent hash: {}",
                            target_block.message.parent_root
                        )
                    })?;
            }
        }

        // If the latest finalized slot is very far back, then only some slots are
        // valid to justify, make sure the target is one of those
        while !is_justifiable_slot(&state.latest_finalized.slot, &target_block.message.slot) {
            target_block = lean_block_provider
                .get(target_block.message.parent_root)?
                .ok_or_else(|| {
                    anyhow!(
                        "Block not found for target block's parent hash: {}",
                        target_block.message.parent_root
                    )
                })?;
        }

        let head_block = lean_block_provider
            .get(self.head)?
            .ok_or_else(|| anyhow!("Block not found for head: {}", self.head))?;

        Ok(Vote {
            slot,
            head: Checkpoint {
                root: self.head,
                slot: head_block.message.slot,
            },
            target: Checkpoint {
                root: target_block.message.tree_hash_root(),
                slot: target_block.message.slot,
            },
            source: state.latest_justified.clone(),
        })
    }
}
