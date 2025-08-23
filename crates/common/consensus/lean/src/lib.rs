pub mod block;
pub mod checkpoint;
pub mod config;
pub mod state;
pub mod vote;

use std::collections::HashMap;

use alloy_primitives::B256;
use anyhow::anyhow;
use ream_metrics::{FINALIZED_SLOT, HEAD_SLOT, JUSTIFIED_SLOT, set_int_gauge_vec};
use serde::{Deserialize, Serialize};

use crate::{
    block::Block,
    state::LeanState,
    vote::{SignedVote, Vote},
};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoteItem {
    Signed(SignedVote),
    Unsigned(Vote),
}

/// We allow justification of slots either <= 5 or a perfect square or oblong after
/// the latest finalized slot. This gives us a backoff technique and ensures
/// finality keeps progressing even under high latency
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

/// Given a state, output the new state after processing that block
pub fn process_block(pre_state: &LeanState, block: &Block) -> anyhow::Result<LeanState> {
    set_int_gauge_vec(&HEAD_SLOT, block.slot as i64, &[]);

    let mut state = pre_state.clone();

    // Track historical blocks in the state
    state
        .historical_block_hashes
        .push(block.parent)
        .map_err(|err| anyhow!("Failed to add block.parent to historical_block_hashes: {err:?}"))?;
    state
        .justified_slots
        .push(false)
        .map_err(|err| anyhow!("Failed to add to justified_slots: {err:?}"))?;

    while state.historical_block_hashes.len() < block.slot as usize {
        state
            .justified_slots
            .push(false)
            .map_err(|err| anyhow!("Failed to prefill justified_slots: {err:?}"))?;

        state
            .historical_block_hashes
            // Diverged from Python implementation: uses `B256::ZERO` instead of `None`
            .push(B256::ZERO)
            .map_err(|err| anyhow!("Failed to prefill historical_block_hashes: {err:?}"))?;
    }

    // Process votes
    for vote in &block.votes {
        // Ignore votes whose source is not already justified,
        // or whose target is not in the history, or whose target is not a
        // valid justifiable slot
        if !state.justified_slots[vote.source.slot as usize]
            || vote.source.root != state.historical_block_hashes[vote.source.slot as usize]
            || vote.target.root != state.historical_block_hashes[vote.target.slot as usize]
            || vote.target.slot <= vote.source.slot
            || !is_justifiable_slot(&state.latest_finalized.slot, &vote.target.slot)
        {
            continue;
        }

        // Track attempts to justify new hashes
        state.initialize_justifications_for_root(&vote.target.root)?;
        state.set_justification(&vote.target.root, &vote.validator_id, true)?;

        let count = state.count_justifications(&vote.target.root)?;

        // If 2/3 voted for the same new valid hash to justify
        if count == (2 * state.config.num_validators) / 3 {
            state.latest_justified.root = vote.target.root;
            state.latest_justified.slot = vote.target.slot;
            state.justified_slots[vote.target.slot as usize] = true;
            set_int_gauge_vec(&JUSTIFIED_SLOT, state.latest_justified.slot as i64, &[]);

            state.remove_justifications(&vote.target.root)?;

            // Finalization: if the target is the next valid justifiable
            // hash after the source
            let is_target_next_valid_justifiable_slot = !((vote.source.slot + 1)..vote.target.slot)
                .any(|slot| is_justifiable_slot(&state.latest_finalized.slot, &slot));

            if is_target_next_valid_justifiable_slot {
                state.latest_finalized.root = vote.source.root;
                state.latest_finalized.slot = vote.source.slot;
                set_int_gauge_vec(&FINALIZED_SLOT, state.latest_finalized.slot as i64, &[]);
            }
        }
    }

    Ok(state)
}

/// Get the highest-slot justified block that we know about
pub fn get_latest_justified_hash(post_states: &HashMap<B256, LeanState>) -> Option<B256> {
    post_states
        .values()
        .max_by_key(|state| state.latest_justified.slot)
        .map(|state| state.latest_justified.root)
}

/// Use LMD GHOST to get the head, given a particular root (usually the
/// latest known justified block)
pub fn get_fork_choice_head(
    blocks: &HashMap<B256, Block>,
    provided_root: &B256,
    votes: &[Vote],
    min_score: u64,
) -> anyhow::Result<B256> {
    let mut root = *provided_root;

    // Start at genesis by default
    if root == B256::ZERO {
        root = blocks
            .iter()
            .min_by_key(|(_, block)| block.slot)
            .map(|(hash, _)| *hash)
            .ok_or_else(|| anyhow!("No blocks found to calculate fork choice"))?;
    }

    // Identify latest votes

    // Sort votes by ascending slots to ensure that new votes are inserted last
    let mut sorted_votes = votes.to_owned();
    sorted_votes.sort_by_key(|vote| vote.slot);

    // Prepare a map of validator_id -> their vote
    let mut latest_votes = HashMap::<u64, Vote>::new();

    for vote in sorted_votes {
        let validator_id = vote.validator_id;
        latest_votes.insert(validator_id, vote.clone());
    }

    // For each block, count the number of votes for that block. A vote
    // for any descendant of a block also counts as a vote for that block
    let mut vote_weights = HashMap::<B256, u64>::new();

    for vote in latest_votes.values() {
        if blocks.contains_key(&vote.head.root) {
            let mut block_hash = vote.head.root;
            while {
                let current_block = blocks
                    .get(&block_hash)
                    .ok_or_else(|| anyhow!("Block not found for vote head: {block_hash}"))?;
                let root_block = blocks
                    .get(&root)
                    .ok_or_else(|| anyhow!("Block not found for root: {root}"))?;
                current_block.slot > root_block.slot
            } {
                let current_weights = vote_weights.get(&block_hash).unwrap_or(&0);
                vote_weights.insert(block_hash, current_weights + 1);
                block_hash = blocks
                    .get(&block_hash)
                    .map(|block| block.parent)
                    .ok_or_else(|| anyhow!("Block not found for block parent: {block_hash}"))?;
            }
        }
    }

    // Identify the children of each block
    let mut children_map = HashMap::<B256, Vec<B256>>::new();

    for (hash, block) in blocks {
        // Original Python impl uses `block.parent` to imply that the block has a parent,
        // So for Rust, we use `block.parent != B256::ZERO` instead.
        if block.parent != B256::ZERO && *vote_weights.get(hash).unwrap_or(&0) >= min_score {
            children_map.entry(block.parent).or_default().push(*hash);
        }
    }

    // Start at the root (latest justified hash or genesis) and repeatedly
    // choose the child with the most latest votes, tiebreaking by slot then hash
    let mut current_root = root;

    while let Some(children) = children_map.get(&current_root) {
        current_root = *children
            .iter()
            .max_by_key(|child_hash| {
                let vote_weight = vote_weights.get(*child_hash).unwrap_or(&0);
                let slot = blocks.get(*child_hash).map(|block| block.slot).unwrap_or(0);
                (*vote_weight, slot, *(*child_hash))
            })
            .ok_or_else(|| anyhow!("No children found for current root: {current_root}"))?;
    }

    Ok(current_root)
}
