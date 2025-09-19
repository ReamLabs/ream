use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use anyhow::{Ok, anyhow};
use ream_consensus_lean::vote::{SignedVote, Vote};
use ream_storage::{db::lean::LeanDB, tables::table::Table};
use tokio::sync::Mutex;

/// Use LMD GHOST to get the head, given a particular root (usually the
/// latest known justified block)
pub async fn get_fork_choice_head(
    store: Arc<Mutex<LeanDB>>,
    votes: &[SignedVote],
    provided_root: &B256,
    min_score: u64,
) -> anyhow::Result<B256> {
    let mut root = *provided_root;

    let (slot_index_table, lean_block_provider) = {
        let db = store.lock().await;
        (db.slot_index_provider(), db.lean_block_provider())
    };

    // Start at genesis by default
    if root == B256::ZERO {
        root = slot_index_table
            .get_oldest_root()?
            .ok_or(anyhow!("No blocks found to calculate fork choice"))?;
    }

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
        if lean_block_provider.contains_key(vote.head.root) {
            let mut block_hash = vote.head.root;
            while {
                let current_block = lean_block_provider
                    .get(block_hash)?
                    .ok_or_else(|| anyhow!("Block not found for vote head: {block_hash}"))?;
                let root_block = lean_block_provider
                    .get(root)?
                    .ok_or_else(|| anyhow!("Block not found for root: {root}"))?;
                current_block.message.slot > root_block.message.slot
            } {
                let current_weights = vote_weights.get(&block_hash).unwrap_or(&0);
                vote_weights.insert(block_hash, current_weights + 1);
                block_hash = lean_block_provider
                    .get(block_hash)?
                    .map(|block| block.message.parent_root)
                    .ok_or_else(|| anyhow!("Block not found for block parent: {block_hash}"))?;
            }
        }
    }

    // Identify the children of each block
    let children_map = lean_block_provider.get_children_map(min_score, &vote_weights)?;

    // Start at the root (latest justified hash or genesis) and repeatedly
    // choose the child with the most latest votes, tiebreaking by slot then hash
    let mut current_root = root;

    while let Some(children) = children_map.get(&current_root) {
        current_root = *children
            .iter()
            .max_by_key(|child_hash| {
                let vote_weight = vote_weights.get(*child_hash).unwrap_or(&0);
                let slot = lean_block_provider
                    .get(**child_hash)
                    .map(|maybe_block| match maybe_block {
                        Some(block) => block.message.slot,
                        None => 0,
                    })
                    .unwrap_or(0);
                (*vote_weight, slot, *(*child_hash))
            })
            .ok_or_else(|| anyhow!("No children found for current root: {current_root}"))?;
    }

    Ok(current_root)
}
