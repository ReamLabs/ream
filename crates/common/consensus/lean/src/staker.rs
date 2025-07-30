use ream_pqc::PublicKey;
use ream_pqc::PQSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};
use tree_hash_derive::TreeHash;

use crate::{
    block::Block,
    get_fork_choice_head,
    get_latest_justified_hash,
    Hash,
    is_justifiable_slot,
    process_block,
    QueueItem,
    state::State,
    vote::{Vote, VoteData},
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Staker {
    pub validator_id: u64,
    pub public_key: PublicKey, // Additional to 3SF-mini
    pub network: Weak<RefCell<P2PNetwork>>,
    pub chain: HashMap<Hash, Block>,
    pub post_states: HashMap<Hash, State>,
    pub known_votes: Vec<Vote>,
    pub new_votes: Vec<Vote>,
    pub dependencies: HashMap<Hash, Vec<QueueItem>>,
    pub genesis_hash: Hash,
    pub num_validators: u64,
    pub safe_target: Option<Hash>,
    pub head: Hash,
}

impl Staker {
    pub fn new(
        validator_id: u64,
        network: &Rc<RefCell<P2PNetwork>>,
        genesis_block: &Block,
        genesis_state: &State,
    ) -> Staker {
        let genesis_hash = genesis_block.compute_hash();
        let mut chain = HashMap::<Hash, Block>::new();
        chain.insert(genesis_hash, genesis_block.clone());

        let mut post_states = HashMap::<Hash, State>::new();
        post_states.insert(genesis_hash, genesis_state.clone());

        Staker {
            validator_id,
            public_key: PublicKey {},
            network: Rc::downgrade(network),
            chain,
            post_states,
            known_votes: Vec::<Vote>::new(),
            new_votes: Vec::<Vote>::new(),
            dependencies: HashMap::<Hash, Vec<QueueItem>>::new(),
            genesis_hash,
            num_validators: genesis_state.config.num_validators,
            safe_target: None,
            head: genesis_hash,
        }
    }

    /// A helper function that returns the Staker's network reference
    /// by abstracting away Reference Counted implementation.
    ///
    /// Using `self.get_network()` is recommended over using `self.network` directly.
    ///
    /// Learn more: https://doc.rust-lang.org/std/rc/
    fn get_network(&self) -> Rc<RefCell<P2PNetwork>> {
        self.network // Weak<RefCell<P2PNetwork>>
            .upgrade() // Option<Rc<RefCell<P2PNetwork>>>
            .unwrap() // Rc<RefCell<P2PNetwork>>
    }

    pub fn latest_justified_hash(&self) -> Option<Hash> {
        get_latest_justified_hash(&self.post_states)
    }

    pub fn latest_finalized_hash(&self) -> Option<Hash> {
        match self.post_states.get(&self.head) {
            Some(state) => {
                Some(state.latest_finalized_hash)
            },
            None => None
        }
    }

    /// Compute the latest block that the staker is allowed to choose as the target
    fn compute_safe_target(&self) -> Hash {
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
            if known_votes
                .find(|known_vote| *known_vote == *new_vote)
                .is_none()
            {
                self.known_votes.push(new_vote.clone());
            }
        }

        self.new_votes.clear();
        self.recompute_head();
    }

    // Done upon processing new votes or a new block
    fn recompute_head(&mut self) {
        let justified_hash = get_latest_justified_hash(&self.post_states).unwrap();
        self.head = get_fork_choice_head(&self.chain, &justified_hash, &self.known_votes, 0);
    }

    // Called every second
    pub fn tick(&mut self) {
        let time_in_slot = self.get_network().borrow().time % SLOT_DURATION;

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
            self.safe_target = Some(self.compute_safe_target());
        // Deadline to accept attestations except for those included in a block
        } else if time_in_slot == SLOT_DURATION * 3 / 4 {
            self.accept_new_votes();
        }
    }

    fn get_current_slot(&self) -> u64 {
        self.get_network().borrow().time / SLOT_DURATION + 2
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
            votes: Vec::new(),
            state_root: None,
        };
        let mut state: State;

        // Keep attempt to add valid votes from the list of available votes
        loop {
            state = process_block(&head_state, &new_block);

            let mut new_votes_to_add = Vec::<Vote>::new();
            for vote in self.known_votes.clone().into_iter() {
                if vote.data.source == state.latest_justified_hash
                    && new_block.votes
                        .clone()
                        .into_iter()
                        .find(|v| *v == vote)
                        .is_none()
                {
                    new_votes_to_add.push(vote);
                }
            }

            if new_votes_to_add.is_empty() {
                break;
            }

            new_block.votes.append(&mut new_votes_to_add);
        }

        new_block.state_root = Some(state.compute_hash());
        let new_hash = new_block.compute_hash();

        self.chain.insert(new_hash, new_block.clone());
        self.post_states.insert(new_hash, state);

        self.get_network()
            .borrow_mut()
            .submit(QueueItem::BlockItem(new_block), self.validator_id);
    }

    // Called when it's the staker's turn to vote
    fn vote(&mut self) {
        let state = self.post_states.get(&self.head).unwrap();
        let mut target_block = self.chain.get(&self.head).unwrap();
        let safe_target = self.safe_target.unwrap_or(self.genesis_hash);

        // If there is no very recent safe target, then vote for the k'th ancestor
        // of the head
        for _ in 0..3 {
            if target_block.slot > self.chain.get(&safe_target).unwrap().slot {
                target_block = self.chain.get(&target_block.parent.unwrap()).unwrap();
            }
        }

        // If the latest finalized slot is very far back, then only some slots are
        // valid to justify, make sure the target is one of those
        while !is_justifiable_slot(&state.latest_finalized_slot, &target_block.slot) {
            target_block = self.chain.get(&target_block.parent.unwrap()).unwrap();
        }

        let vote_data = VoteData {
            validator_id: self.validator_id,
            slot: self.get_current_slot(),
            head: self.head,
            head_slot: self.chain.get(&self.head).unwrap().slot,
            target: target_block.compute_hash(),
            target_slot: target_block.slot,
            source: state.latest_justified_hash,
            source_slot: state.latest_justified_slot,
        };

        let vote = Vote {
            data: vote_data,
            signature: PQSignature {},
        };

        println!(
            "voting (Staker {}), head = {}, t = {}, s = {}",
            self.validator_id,
            &self.chain.get(&self.head).unwrap().slot,
            &target_block.slot,
            &state.latest_justified_slot
        );

        self.receive(&QueueItem::VoteItem(vote.clone()));

        self.get_network()
            .borrow_mut()
            .submit(QueueItem::VoteItem(vote), self.validator_id);
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
                        let state = process_block(&parent_state, &block);

                        self.chain.insert(block_hash, block.clone());
                        self.post_states.insert(block_hash, state);

                        let mut known_votes = self.known_votes.clone().into_iter();

                        for vote in &block.votes {
                            if known_votes
                                .find(|known_vote| *known_vote == *vote)
                                .is_none()
                            {
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
                            .entry(block.parent.unwrap())
                            .or_insert_with(Vec::new)
                            .push(queue_item.clone());
                    }
                }
            }
            QueueItem::VoteItem(vote) => {
                let is_known_vote = self
                    .known_votes
                    .clone()
                    .into_iter()
                    .find(|known_vote| known_vote == vote)
                    .is_some();
                let is_new_vote = self
                    .new_votes
                    .clone()
                    .into_iter()
                    .find(|new_vote| new_vote == vote)
                    .is_some();

                if is_known_vote || is_new_vote {
                    return;
                } else if self.chain.contains_key(&vote.data.head) {
                    self.new_votes.push(vote.clone());
                } else {
                    self.dependencies
                        .entry(vote.data.head)
                        .or_insert_with(Vec::new)
                        .push(queue_item.clone());
                }
            }
        }
    }
}
