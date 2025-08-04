use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use alloy_primitives::B256;
use ream_consensus_lean::{
    QueueItem, SLOT_DURATION,
    block::Block,
    get_fork_choice_head, get_latest_justified_hash, is_justifiable_slot, process_block,
    state::LeanState,
    vote::{SignedVote, Vote},
};
use ream_p2p::network::lean::NetworkService;
use ream_pqc::PQSignature;
use ssz_types::VariableList;
use tracing::info;
use tree_hash::TreeHash;

pub struct Staker {
    pub validator_id: u64,
    pub chain: HashMap<B256, Block>,
    pub network: Arc<Mutex<NetworkService>>,
    pub post_states: HashMap<B256, LeanState>,
    pub known_votes: Vec<Vote>,
    pub new_votes: Vec<Vote>,
    pub dependencies: HashMap<B256, Vec<QueueItem>>,
    pub genesis_hash: B256,
    pub num_validators: u64,
    pub safe_target: B256,
    pub head: B256,
}

impl Staker {
    pub fn new(
        validator_id: u64,
        network: Arc<Mutex<NetworkService>>,
        genesis_block: Block,
        genesis_state: LeanState,
    ) -> Staker {
        let genesis_hash = genesis_block.tree_hash_root();

        Staker {
            // This node's validator ID
            validator_id,
            // Hook to the p2p network
            network,
            // Votes that we have received and taken into account
            known_votes: Vec::new(),
            // Votes that we have received but not yet taken into account
            new_votes: Vec::new(),
            // Objects that we will process once we have processed their parents
            dependencies: HashMap::new(),
            // Initialize the chain with the genesis block
            genesis_hash,
            num_validators: genesis_state.config.num_validators,
            // Block that it is safe to use to vote as the target
            // Diverge from Python implementation: Use genesis hash instead of `None`
            safe_target: genesis_hash,
            // Head of the chain
            head: genesis_hash,
            // {block_hash: block} for all blocks that we know about
            chain: HashMap::from([(genesis_hash, genesis_block)]),
            // {block_hash: post_state} for all blocks that we know about
            post_states: HashMap::from([(genesis_hash, genesis_state)]),
        }
    }

    pub fn latest_justified_hash(&self) -> Option<B256> {
        get_latest_justified_hash(&self.post_states)
    }

    pub fn latest_finalized_hash(&self) -> Option<B256> {
        self.post_states
            .get(&self.head)
            .map(|state| state.latest_finalized_hash)
    }

    /// Compute the latest block that the staker is allowed to choose as the target
    fn compute_safe_target(&self) -> B256 {
        let justified_hash = get_latest_justified_hash(&self.post_states).unwrap();

        get_fork_choice_head(
            &self.chain,
            &justified_hash,
            &self.new_votes,
            self.num_validators * 2 / 3,
        )
    }

    /// Process new votes that the staker has received. Vote processing is done
    /// at a particular time, because of safe target and view merge rule
    fn accept_new_votes(&mut self) {
        for new_vote in &self.new_votes {
            if !self.known_votes.contains(new_vote) {
                self.known_votes.push(new_vote.clone());
            }
        }

        self.new_votes = Vec::new();
        self.recompute_head();
    }

    /// Done upon processing new votes or a new block
    fn recompute_head(&mut self) {
        let justified_hash = get_latest_justified_hash(&self.post_states)
            .expect("Failed to get latest_justified_hash from post_states");
        self.head = get_fork_choice_head(&self.chain, &justified_hash, &self.known_votes, 0);
    }

    /// Called every second
    pub fn tick(&mut self) {
        let time_in_slot = self.network.lock().unwrap().time % SLOT_DURATION;

        // t=0: propose a block
        if time_in_slot == 0 {
            if self.get_current_slot() % self.num_validators == self.validator_id {
                // View merge mechanism: a node accepts attestations that it received
                // <= 1/4 before slot start, or attestations in the latest block
                self.accept_new_votes();
                self.propose_block();
            }
        // t=1/4: vote
        } else if time_in_slot == SLOT_DURATION / 4 {
            self.vote();
        // t=2/4: compute the safe target (this must be done here to ensure
        // that, assuming network latency assumptions are satisfied, anything that
        // one honest node receives by this time, every honest node will receive by
        // the general attestation deadline)
        } else if time_in_slot == SLOT_DURATION * 2 / 4 {
            self.safe_target = self.compute_safe_target();
        // Deadline to accept attestations except for those included in a block
        } else if time_in_slot == SLOT_DURATION * 3 / 4 {
            self.accept_new_votes();
        }
    }

    fn get_current_slot(&self) -> u64 {
        self.network.lock().unwrap().time / SLOT_DURATION + 2
    }

    /// Called when it's the staker's turn to propose a block
    fn propose_block(&mut self) {
        let new_slot = self.get_current_slot();

        info!(
            "proposing (Staker {}), head = {}",
            self.validator_id,
            self.chain.get(&self.head).unwrap().slot
        );

        let head_state = self.post_states.get(&self.head).unwrap();
        let mut new_block = Block {
            slot: new_slot,
            parent: self.head,
            votes: VariableList::empty(),
            // Diverged from Python implementation: Using `B256::ZERO` instead of `None`)
            state_root: B256::ZERO,
        };
        let mut state: LeanState;

        // Keep attempt to add valid votes from the list of available votes
        loop {
            state = process_block(head_state, &new_block);

            let new_votes_to_add = self
                .known_votes
                .clone()
                .into_iter()
                .filter(|vote| vote.source == state.latest_justified_hash)
                .filter(|vote| !new_block.votes.contains(vote))
                .collect::<Vec<_>>();

            if new_votes_to_add.is_empty() {
                break;
            }

            for vote in new_votes_to_add {
                // TODO: proper error handling
                new_block
                    .votes
                    .push(vote)
                    .expect("Failed to add vote to new_block");
            }
        }

        new_block.state_root = state.tree_hash_root();
        let new_hash = new_block.tree_hash_root();

        self.chain.insert(new_hash, new_block.clone());
        self.post_states.insert(new_hash, state);

        // TODO: submit to actual network
        // self.get_network()
        //     .borrow_mut()
        //     .submit(QueueItem::BlockItem(new_block), self.validator_id);
    }

    /// Called when it's the staker's turn to vote
    fn vote(&mut self) {
        let state = self.post_states.get(&self.head).unwrap();
        let mut target_block = self.chain.get(&self.head).unwrap();

        // If there is no very recent safe target, then vote for the k'th ancestor
        // of the head
        for _ in 0..3 {
            if target_block.slot > self.chain.get(&self.safe_target).unwrap().slot {
                target_block = self.chain.get(&target_block.parent).unwrap();
            }
        }

        // If the latest finalized slot is very far back, then only some slots are
        // valid to justify, make sure the target is one of those
        while !is_justifiable_slot(&state.latest_finalized_slot, &target_block.slot) {
            target_block = self.chain.get(&target_block.parent).unwrap();
        }

        let vote = Vote {
            validator_id: self.validator_id,
            slot: self.get_current_slot(),
            head: self.head,
            head_slot: self.chain.get(&self.head).unwrap().slot,
            target: target_block.tree_hash_root(),
            target_slot: target_block.slot,
            source: state.latest_justified_hash,
            source_slot: state.latest_justified_slot,
        };

        let signed_vote = SignedVote {
            data: vote,
            signature: PQSignature {},
        };

        info!(
            "voting (Staker {}), head = {}, t = {}, s = {}",
            self.validator_id,
            &self.chain.get(&self.head).unwrap().slot,
            &target_block.slot,
            &state.latest_justified_slot
        );

        self.receive(&QueueItem::VoteItem(signed_vote.clone()));

        // TODO: submit to actual network
        // self.get_network()
        //     .borrow_mut()
        //     .submit(QueueItem::VoteItem(vote), self.validator_id);
    }

    /// Called by the p2p network
    fn receive(&mut self, queue_item: &QueueItem) {
        match queue_item {
            QueueItem::BlockItem(block) => {
                let block_hash = block.tree_hash_root();

                // If the block is already known, ignore it
                if self.chain.contains_key(&block_hash) {
                    return;
                }

                match self.post_states.get(&block.parent) {
                    Some(parent_state) => {
                        let state = process_block(parent_state, block);

                        self.chain.insert(block_hash, block.clone());
                        self.post_states.insert(block_hash, state);

                        let mut known_votes = self.known_votes.clone().into_iter();

                        for vote in &block.votes {
                            if !known_votes.any(|known_vote| known_vote == *vote) {
                                self.known_votes.push(vote.clone());
                            }
                        }

                        self.recompute_head();

                        // Once we have received a block, also process all of
                        // its dependencies
                        if let Some(queue_items) = self.dependencies.get(&block_hash) {
                            for item in queue_items.clone() {
                                self.receive(&item);
                            }

                            self.dependencies.remove(&block_hash);
                        }
                    }
                    None => {
                        // If we have not yet seen the block's parent, ignore for now,
                        // process later once we actually see the parent
                        self.dependencies
                            .entry(block.parent)
                            .or_default()
                            .push(queue_item.clone());
                    }
                }
            }
            QueueItem::VoteItem(vote) => {
                let is_known_vote = self
                    .known_votes
                    .clone()
                    .into_iter()
                    .any(|known_vote| known_vote == vote.data);

                let is_new_vote = self
                    .new_votes
                    .clone()
                    .into_iter()
                    .any(|new_vote| new_vote == vote.data);

                if is_known_vote || is_new_vote {
                    // Do nothing
                } else if self.chain.contains_key(&vote.data.head) {
                    self.new_votes.push(vote.data.clone());
                } else {
                    self.dependencies
                        .entry(vote.data.head)
                        .or_default()
                        .push(queue_item.clone());
                }
            }
        }
    }
}
