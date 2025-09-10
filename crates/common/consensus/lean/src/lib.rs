pub mod block;
pub mod checkpoint;
pub mod config;
pub mod state;
pub mod vote;

use std::collections::HashMap;

use alloy_primitives::B256;
use anyhow::anyhow;

use crate::{
    block::Block,
    state::LeanState,
    vote::{SignedVote, Vote},
};

/// We allow justification of slots either <= 5 or a perfect square or oblong after
/// the latest finalized slot. This gives us a backoff technique and ensures
/// finality keeps progressing even under high latency
pub fn is_justifiable_slot(finalized_slot: &u64, candidate_slot: &u64) -> bool {
    assert!(
        candidate_slot >= finalized_slot,
        "Candidate slot ({candidate_slot}) must be more than or equal to finalized slot ({finalized_slot})"
    );

    let delta = candidate_slot - finalized_slot;

    delta <= 5
    || (delta as f64).sqrt().fract() == 0.0 // any x^2
    || (delta as f64 + 0.25).sqrt() % 1.0 == 0.5 // any x^2+x
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
    votes: &[SignedVote],
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
    sorted_votes.sort_by_key(|signed_vote| signed_vote.message.slot);

    // Prepare a map of validator_id -> their vote
    let mut latest_votes = HashMap::<u64, Vote>::new();

    for signed_vote in sorted_votes {
        let validator_id = signed_vote.validator_id;
        latest_votes.insert(validator_id, signed_vote.message.clone());
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
                    .map(|block| block.parent_root)
                    .ok_or_else(|| anyhow!("Block not found for block parent: {block_hash}"))?;
            }
        }
    }

    // Identify the children of each block
    let mut children_map = HashMap::<B256, Vec<B256>>::new();

    for (hash, block) in blocks {
        // Original Python impl uses `block.parent` to imply that the block has a parent,
        // So for Rust, we use `block.parent != B256::ZERO` instead.
        if block.parent_root != B256::ZERO && *vote_weights.get(hash).unwrap_or(&0) >= min_score {
            children_map
                .entry(block.parent_root)
                .or_default()
                .push(*hash);
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
