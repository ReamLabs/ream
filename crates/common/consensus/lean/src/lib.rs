pub mod block;
pub mod staker;
pub mod state;
pub mod vote;

use std::collections::HashMap;

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_types::{
    VariableList,
    typenum::{
        U16777216, // 2**24
    },
};

use crate::{block::Block, state::LeanState, vote::Vote};

pub type Hash = B256;

pub const ZERO_HASH: Hash = Hash::ZERO;
pub const SLOT_DURATION: usize = 12;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum QueueItem {
    BlockItem(Block),
    VoteItem(Vote),
}

// We allow justification of slots either <= 5 or a perfect square or oblong after
// the latest finalized slot. This gives us a backoff technique and ensures
// finality keeps progressing even under high latency
pub fn is_justifiable_slot(finalized_slot: &usize, candidate_slot: &usize) -> bool {
    assert!(
        candidate_slot >= finalized_slot,
        "Candidate slot ({candidate_slot}) is less than finalized slot ({finalized_slot})"
    );

    let delta = candidate_slot - finalized_slot;

    delta <= 5
    || (delta as f64).sqrt().fract() == 0.0 // any x^2
    || (delta as f64 + 0.25).sqrt() % 1.0 == 0.5 // any x^2+x
}

// Given a state, output the new state after processing that block
pub fn process_block(pre_state: &LeanState, block: &Block) -> LeanState {
    let mut state = pre_state.clone();

    // Track historical blocks in the state
    // TODO: proper error handlings
    let _ = state.historical_block_hashes.push(block.parent);
    let _ = state.justified_slots.push(false);

    while state.historical_block_hashes.len() < block.slot {
        // TODO: proper error handlings
        let _ = state.justified_slots.push(false);
        let _ = state.historical_block_hashes.push(None);
    }

    // Process votes
    for vote in &block.votes {
        // Ignore votes whose source is not already justified,
        // or whose target is not in the history, or whose target is not a
        // valid justifiable slot
        if !state.justified_slots[vote.data.source_slot]
            || Some(vote.data.source) != state.historical_block_hashes[vote.data.source_slot]
            || Some(vote.data.target) != state.historical_block_hashes[vote.data.target_slot]
            || vote.data.target_slot <= vote.data.source_slot
            || !is_justifiable_slot(&state.latest_finalized_slot, &vote.data.target_slot)
        {
            continue;
        }

        // Track attempts to justify new hashes
        if !state.justifications.contains_key(&vote.data.target) {
            let mut empty_justifications = Vec::<bool>::with_capacity(state.num_validators);
            empty_justifications.resize(state.num_validators, false);

            state
                .justifications
                .insert(vote.data.target, empty_justifications);
        }

        if !state.justifications[&vote.data.target][vote.data.validator_id] {
            state.justifications.get_mut(&vote.data.target).unwrap()[vote.data.validator_id] = true;
        }

        let count = state.justifications[&vote.data.target]
            .iter()
            .fold(0, |sum, justification| sum + *justification as usize);

        // If 2/3 voted for the same new valid hash to justify
        if count == (2 * state.num_validators) / 3 {
            state.latest_justified_hash = vote.data.target;
            state.latest_justified_slot = vote.data.target_slot;
            state.justified_slots[vote.data.target_slot] = true;

            state.justifications.remove(&vote.data.target).unwrap();

            // Finalization: if the target is the next valid justifiable
            // hash after the source
            let mut is_target_next_valid_justifiable_slot = true;

            for slot in (vote.data.source_slot + 1)..vote.data.target_slot {
                if is_justifiable_slot(&state.latest_finalized_slot, &slot) {
                    is_target_next_valid_justifiable_slot = false;
                    break;
                }
            }

            if is_target_next_valid_justifiable_slot {
                state.latest_finalized_hash = vote.data.source;
                state.latest_finalized_slot = vote.data.source_slot;
            }
        }
    }

    state
}

// Get the highest-slot justified block that we know about
pub fn get_latest_justified_hash(post_states: &HashMap<Hash, LeanState>) -> Option<Hash> {
    post_states
        .values()
        .max_by_key(|state| state.latest_justified_slot)
        .map(|state| state.latest_justified_hash)
}

// Use LMD GHOST to get the head, given a particular root (usually the
// latest known justified block)
pub fn get_fork_choice_head(
    blocks: &HashMap<Hash, Block>,
    provided_root: &Hash,
    votes: &VariableList<Vote, U16777216>,
    min_score: usize,
) -> Hash {
    let mut root = *provided_root;

    // Start at genesis by default
    if *root == ZERO_HASH {
        root = blocks
            .iter()
            .min_by_key(|(_, block)| block.slot)
            .map(|(hash, _)| *hash)
            .unwrap();
    }

    // Sort votes by ascending slots to ensure that new votes are inserted last
    let mut sorted_votes = votes.clone();
    sorted_votes.sort_by_key(|vote| vote.data.slot);

    // Prepare a map of validator_id -> their vote
    let mut latest_votes = HashMap::<usize, Vote>::new();

    for vote in votes {
        let validator_id = vote.data.validator_id;
        latest_votes.insert(validator_id, vote.clone());
    }

    // For each block, count the number of votes for that block. A vote
    // for any descendant of a block also counts as a vote for that block
    let mut vote_weights = HashMap::<Hash, usize>::new();

    for vote in latest_votes.values() {
        if blocks.contains_key(&vote.data.head) {
            let mut block_hash = vote.data.head;
            while blocks.get(&block_hash).unwrap().slot > blocks.get(&root).unwrap().slot {
                let current_weights = vote_weights.get(&block_hash).unwrap_or(&0);
                vote_weights.insert(block_hash, current_weights + 1);
                block_hash = blocks.get(&block_hash).unwrap().parent.unwrap();
            }
        }
    }

    // Identify the children of each block
    let mut children_map = HashMap::<Hash, Vec<Hash>>::new();

    for (hash, block) in blocks {
        if block.parent.is_some() && *vote_weights.get(hash).unwrap_or(&0) >= min_score {
            match children_map.get_mut(&block.parent.unwrap()) {
                Some(child_hashes) => {
                    child_hashes.push(*hash);
                }
                None => {
                    children_map.insert(block.parent.unwrap(), vec![*hash]);
                }
            }
        }
    }

    // Start at the root (latest justified hash or genesis) and repeatedly
    // choose the child with the most latest votes, tiebreaking by slot then hash
    let mut current_root = root;

    loop {
        match children_map.get(&current_root) {
            None => {
                break current_root;
            }
            Some(children) => {
                current_root = *children
                    .iter()
                    .max_by_key(|child_hash| {
                        let vote_weight = vote_weights.get(*child_hash).unwrap_or(&0);
                        let slot = blocks.get(*child_hash).unwrap().slot;
                        (*vote_weight, slot, *(*child_hash))
                    })
                    .unwrap();
            }
        }
    }
}
