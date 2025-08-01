use alloy_primitives::B256;
use ream_pqc::{PQSignature, PublicKey};
use serde::{Deserialize, Serialize};
use ssz_types::{typenum::U4096, VariableList};
use std::collections::HashMap;

use crate::{
    QueueItem, SLOT_DURATION,
    block::Block,
    get_fork_choice_head, get_latest_justified_hash, is_justifiable_slot, process_block,
    state::LeanState,
    vote::{SignedVote, Vote},
};

// TODO: Split to Staker and Service
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Staker {
    pub validator_id: u64,
    pub public_key: PublicKey, // Additional to 3SF-mini
    pub chain: HashMap<B256, Block>,
    pub time: u64, // TODO: update the time so on_tick() works properly
    // TODO: Add back proper networking instead
    // pub network: Weak<RefCell<P2PNetwork>>,
    pub post_states: HashMap<B256, LeanState>,
    pub known_votes: VariableList<Vote, U4096>,
    pub new_votes: VariableList<Vote, U4096>,
    pub dependencies: HashMap<B256, Vec<QueueItem>>,
    pub genesis_hash: B256,
    // TODO: Proper validator key handling from static config
    pub num_validators: u64,
    pub safe_target: B256,
    pub head: B256,
}

impl Staker {
    pub fn new(validator_id: u64, genesis_block: &Block, genesis_state: &LeanState) -> Staker {
        let genesis_hash = genesis_block.compute_hash();
        let mut chain = HashMap::<B256, Block>::new();
        chain.insert(genesis_hash, genesis_block.clone());

        let mut post_states = HashMap::<B256, LeanState>::new();
        post_states.insert(genesis_hash, genesis_state.clone());

        Staker {
            validator_id,
            public_key: PublicKey {},
            chain,
            time: 0,
            post_states,
            known_votes: VariableList::<Vote, U4096>::empty(),
            new_votes: VariableList::<Vote, U4096>::empty(),
            dependencies: HashMap::<B256, Vec<QueueItem>>::new(),
            genesis_hash,
            num_validators: genesis_state.config.num_validators,
            safe_target: genesis_hash,
            head: genesis_hash,
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
        let mut known_votes = self.known_votes.clone().into_iter();

        for new_vote in &self.new_votes {
            if !known_votes.any(|known_vote| known_vote == *new_vote) {
                // TODO: proper error handling
                let _ = self.known_votes.push(new_vote.clone());
            }
        }

        self.new_votes = VariableList::empty();
        self.recompute_head();
    }

    // Done upon processing new votes or a new block
    fn recompute_head(&mut self) {
        let justified_hash = get_latest_justified_hash(&self.post_states).unwrap();
        self.head = get_fork_choice_head(&self.chain, &justified_hash, &self.known_votes, 0);
    }

    // Called every second
    pub fn tick(&mut self) {
        let time_in_slot = self.time % SLOT_DURATION;

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
        self.time / SLOT_DURATION + 2
    }

    // Called when it's the staker's turn to propose a block
    fn propose_block(&mut self) {
        let new_slot = self.get_current_slot();

        println!(
            "proposing (Staker {}), head = {}",
            self.validator_id,
            self.chain.get(&self.head).unwrap().slot
        );

        let head_state = self.post_states.get(&self.head).unwrap();
        let mut new_block = Block {
            slot: new_slot,
            parent: Some(self.head),
            votes: VariableList::empty(),
            state_root: None,
        };
        let mut state: LeanState;

        // Keep attempt to add valid votes from the list of available votes
        loop {
            state = process_block(head_state, &new_block);

            let mut new_votes_to_add = Vec::<Vote>::new();
            for vote in self.known_votes.clone().into_iter() {
                if vote.source == state.latest_justified_hash
                    && !new_block.votes.clone().into_iter().any(|v| v == vote)
                {
                    new_votes_to_add.push(vote);
                }
            }

            if new_votes_to_add.is_empty() {
                break;
            }

            for vote in new_votes_to_add {
                // TODO: proper error handling
                let _ = new_block.votes.push(vote);
            }
        }

        new_block.state_root = Some(state.compute_hash());
        let new_hash = new_block.compute_hash();

        self.chain.insert(new_hash, new_block.clone());
        self.post_states.insert(new_hash, state);

        // TODO: submit to actual network
        // self.get_network()
        //     .borrow_mut()
        //     .submit(QueueItem::BlockItem(new_block), self.validator_id);
    }

    // Called when it's the staker's turn to vote
    fn vote(&mut self) {
        let state = self.post_states.get(&self.head).unwrap();
        let mut target_block = self.chain.get(&self.head).unwrap();

        // If there is no very recent safe target, then vote for the k'th ancestor
        // of the head
        for _ in 0..3 {
            if target_block.slot > self.chain.get(&self.safe_target).unwrap().slot {
                target_block = self.chain.get(&target_block.parent.unwrap()).unwrap();
            }
        }

        // If the latest finalized slot is very far back, then only some slots are
        // valid to justify, make sure the target is one of those
        while !is_justifiable_slot(&state.latest_finalized_slot, &target_block.slot) {
            target_block = self.chain.get(&target_block.parent.unwrap()).unwrap();
        }

        let vote = Vote {
            validator_id: self.validator_id,
            slot: self.get_current_slot(),
            head: self.head,
            head_slot: self.chain.get(&self.head).unwrap().slot,
            target: target_block.compute_hash(),
            target_slot: target_block.slot,
            source: state.latest_justified_hash,
            source_slot: state.latest_justified_slot,
        };

        let signed_vote = SignedVote {
            data: vote,
            signature: PQSignature {},
        };

        println!(
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

    // Called by the p2p network
    fn receive(&mut self, queue_item: &QueueItem) {
        match queue_item {
            QueueItem::BlockItem(block) => {
                let block_hash = block.compute_hash();

                // If the block is already known, ignore it
                if self.chain.contains_key(&block_hash) {
                    return;
                }

                match self.post_states.get(&block.parent.unwrap()) {
                    Some(parent_state) => {
                        let state = process_block(parent_state, block);

                        self.chain.insert(block_hash, block.clone());
                        self.post_states.insert(block_hash, state);

                        let mut known_votes = self.known_votes.clone().into_iter();

                        for vote in &block.votes {
                            if !known_votes.any(|known_vote| known_vote == *vote) {
                                // TODO: proper error handling
                                let _ = self.known_votes.push(vote.clone());
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
                            .entry(block.parent.unwrap())
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
                    // TODO: proper error handling
                    let _ = self.new_votes.push(vote.data.clone());
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
