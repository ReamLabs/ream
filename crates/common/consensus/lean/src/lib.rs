pub mod block;
pub mod config;
pub mod staker;
pub mod state;
pub mod vote;

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_types::{typenum::U4096, VariableList};
use std::collections::HashMap;

use crate::{block::Block, state::LeanState, vote::SignedVote, vote::Vote};

pub const SLOT_DURATION: u64 = 12;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum QueueItem {
    BlockItem(Block),
    VoteItem(SignedVote),
}

// We allow justification of slots either <= 5 or a perfect square or oblong after
// the latest finalized slot. This gives us a backoff technique and ensures
// finality keeps progressing even under high latency
pub fn is_justifiable_slot(finalized_slot: &u64, candidate_slot: &u64) -> bool {
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

    while state.historical_block_hashes.len() < block.slot as usize {
        // TODO: proper error handlings
        let _ = state.justified_slots.push(false);
        let _ = state.historical_block_hashes.push(None);
    }

    // Process votes
    for vote in &block.votes {
        // Ignore votes whose source is not already justified,
        // or whose target is not in the history, or whose target is not a
        // valid justifiable slot
        if !state.justified_slots[vote.source_slot as usize]
            || Some(vote.source) != state.historical_block_hashes[vote.source_slot as usize]
            || Some(vote.target) != state.historical_block_hashes[vote.target_slot as usize]
            || vote.target_slot <= vote.source_slot
            || !is_justifiable_slot(&state.latest_finalized_slot, &vote.target_slot)
        {
            continue;
        }

        // Track attempts to justify new hashes
        if !state.justifications.contains_key(&vote.target) {
            state
                .justifications
                .insert(vote.target, vec![false; state.config.num_validators as usize]);
        }

        if !state.justifications[&vote.target][vote.validator_id as usize] {
            state.justifications.get_mut(&vote.target).unwrap()[vote.validator_id as usize] = true;
        }

        let count = state.justifications[&vote.target]
            .iter()
            .fold(0, |sum, justification| sum + *justification as usize);

        // If 2/3 voted for the same new valid hash to justify
        if count == (2 * state.config.num_validators as usize) / 3 {
            state.latest_justified_hash = vote.target;
            state.latest_justified_slot = vote.target_slot;
            state.justified_slots[vote.target_slot as usize] = true;

            state.justifications.remove(&vote.target).unwrap();

            // Finalization: if the target is the next valid justifiable
            // hash after the source
            let is_target_next_valid_justifiable_slot = !((vote.source_slot + 1)
                ..vote.target_slot)
                .any(|slot| is_justifiable_slot(&state.latest_finalized_slot, &slot));

            if is_target_next_valid_justifiable_slot {
                state.latest_finalized_hash = vote.source;
                state.latest_finalized_slot = vote.source_slot;
            }
        }
    }

    state
}

// Get the highest-slot justified block that we know about
pub fn get_latest_justified_hash(post_states: &HashMap<B256, LeanState>) -> Option<B256> {
    post_states
        .values()
        .max_by_key(|state| state.latest_justified_slot)
        .map(|state| state.latest_justified_hash)
}

// Use LMD GHOST to get the head, given a particular root (usually the
// latest known justified block)
pub fn get_fork_choice_head(
    blocks: &HashMap<B256, Block>,
    provided_root: &B256,
    votes: &VariableList<Vote, U4096>,
    min_score: u64,
) -> B256 {
    let mut root = *provided_root;

    // Start at genesis by default
    if *root == B256::ZERO {
        root = blocks
            .iter()
            .min_by_key(|(_, block)| block.slot)
            .map(|(hash, _)| *hash)
            .unwrap();
    }

    // Sort votes by ascending slots to ensure that new votes are inserted last
    let mut sorted_votes = votes.clone();
    sorted_votes.sort_by_key(|vote| vote.slot);

    // Prepare a map of validator_id -> their vote
    let mut latest_votes = HashMap::<u64, Vote>::new();

    for vote in votes {
        let validator_id = vote.validator_id;
        latest_votes.insert(validator_id, vote.clone());
    }

    // For each block, count the number of votes for that block. A vote
    // for any descendant of a block also counts as a vote for that block
    let mut vote_weights = HashMap::<B256, u64>::new();

    for vote in latest_votes.values() {
        if blocks.contains_key(&vote.head) {
            let mut block_hash = vote.head;
            while blocks.get(&block_hash).unwrap().slot > blocks.get(&root).unwrap().slot {
                let current_weights = vote_weights.get(&block_hash).unwrap_or(&0);
                vote_weights.insert(block_hash, current_weights + 1);
                block_hash = blocks.get(&block_hash).unwrap().parent.unwrap();
            }
        }
    }

    // Identify the children of each block
    let mut children_map = HashMap::<B256, Vec<B256>>::new();

    for (hash, block) in blocks {
        if block.parent.is_some() && *vote_weights.get(hash).unwrap_or(&0) >= min_score {
            children_map
                .entry(block.parent.unwrap())
                .or_insert_with(Vec::new)
                .push(*hash);
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
