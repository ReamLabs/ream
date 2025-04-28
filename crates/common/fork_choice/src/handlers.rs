use alloy_primitives::{B256, map::HashSet};
use anyhow::ensure;
use ream_consensus::{
    attestation::Attestation,
    attester_slashing::AttesterSlashing,
    constants::{INTERVALS_PER_SLOT, SECONDS_PER_SLOT},
    electra::beacon_block::SignedBeaconBlock,
    execution_engine::engine_trait::ExecutionApi,
    misc::compute_start_slot_at_epoch,
    predicates::is_slashable_attestation_data,
};
use tree_hash::TreeHash;

use crate::store::Store;

/// Run ``on_block`` upon receiving a new block.
pub async fn on_block(
    store: &mut Store,
    signed_block: &SignedBeaconBlock,
    execution_engine: &impl ExecutionApi,
) -> anyhow::Result<()> {
    let block = &signed_block.message;

    // Parent block must be known
    ensure!(
        store.block_states.contains_key(&block.parent_root),
        "Missing parent block state for parent_root: {:x}",
        block.parent_root
    );

    // Blocks cannot be in the future. If they are, their consideration must be delayed until they
    // are in the past.
    ensure!(
        store.get_current_slot() >= block.slot,
        "Block slot is ahead of current slot: block.slot = {}, store.get_current_slot() = {}",
        block.slot,
        store.get_current_slot()
    );

    // Check that block is later than the finalized epoch slot (optimization to reduce calls to
    // get_ancestor)
    let finalized_slot = compute_start_slot_at_epoch(store.finalized_checkpoint.epoch);
    ensure!(block.slot > finalized_slot);

    // Check block is a descendant of the finalized block at the checkpoint finalized slot
    let finalized_checkpoint_block =
        store.get_checkpoint_block(block.parent_root, store.finalized_checkpoint.epoch)?;

    ensure!(store.finalized_checkpoint.root == finalized_checkpoint_block);

    // Check if blob data is available
    // If not, this block MAY be queued and subsequently considered when blob data becomes available
    // *Note*: Extraneous or invalid Blobs (in addition to the expected/referenced valid blobs)
    // received on the p2p network MUST NOT invalidate a block that is otherwise valid and available
    ensure!(
        store
            .is_data_available(
                &block.body.blob_kzg_commitments,
                execution_engine,
                block.tree_hash_root()
            )
            .await?
    );

    // Check the block is valid and compute the post-state
    // Make a copy of the state to avoid mutability issues
    let mut state = store.block_states[&block.parent_root].clone();
    let block_root = block.tree_hash_root();
    state
        .state_transition(signed_block, true, execution_engine)
        .await?;

    // Add new block to the store
    store.blocks.insert(block_root, block.clone());
    // Add new state for this block to the store
    store.block_states.insert(block_root, state.clone());

    // Add block timeliness to the store
    let time_into_slot = (store.time - store.genesis_time) % SECONDS_PER_SLOT;
    let is_before_attesting_interval = time_into_slot < SECONDS_PER_SLOT / INTERVALS_PER_SLOT;
    let is_timely = store.get_current_slot() == block.slot && is_before_attesting_interval;
    store
        .block_timeliness
        .insert(block.tree_hash_root(), is_timely);

    // Add proposer score boost if the block is timely and not conflicting with an existing block
    let is_first_block = store.proposer_boost_root == B256::ZERO;
    if is_timely && is_first_block {
        store.proposer_boost_root = block.tree_hash_root()
    }

    // Update checkpoints in store if necessary
    store.update_checkpoints(
        state.current_justified_checkpoint,
        state.finalized_checkpoint,
    );

    // Eagerly compute unrealized justification and finality.
    store.compute_pulled_up_tip(block_root)?;

    Ok(())
}

/// Run ``on_attester_slashing`` immediately upon receiving a new ``AttesterSlashing``
/// from either within a block or directly on the wire.
pub fn on_attester_slashing(
    store: &mut Store,
    attester_slashing: AttesterSlashing,
) -> anyhow::Result<()> {
    let attestation_1 = attester_slashing.attestation_1;
    let attestation_2 = attester_slashing.attestation_2;
    ensure!(is_slashable_attestation_data(
        &attestation_1.data,
        &attestation_2.data
    ));
    let state = &store.block_states[&store.justified_checkpoint.root];
    ensure!(state.is_valid_indexed_attestation(&attestation_1)?);
    ensure!(state.is_valid_indexed_attestation(&attestation_2)?);

    let attestation_1_indices = attestation_1
        .attesting_indices
        .into_iter()
        .collect::<HashSet<_>>();
    let attestation_2_indices = attestation_2
        .attesting_indices
        .into_iter()
        .collect::<HashSet<_>>();
    for index in attestation_1_indices.intersection(&attestation_2_indices) {
        store.equivocating_indices.push(*index);
    }
    Ok(())
}

pub fn on_tick(store: &mut Store, time: u64) -> anyhow::Result<()> {
    // If the ``store.time`` falls behind, while loop catches up slot by slot
    // to ensure that every previous slot is processed with ``on_tick_per_slot``
    let tick_slot = (time - store.genesis_time) / SECONDS_PER_SLOT;
    while store.get_current_slot() < tick_slot {
        let previous_time = store.genesis_time + (store.get_current_slot() + 1) * SECONDS_PER_SLOT;
        store.on_tick_per_slot(previous_time)?;
    }

    store.on_tick_per_slot(time)?;

    Ok(())
}

/// Run ``on_attestation`` upon receiving a new ``attestation`` from either within a block or
/// directly on the wire.
///
/// An ``attestation`` that is asserted as invalid may be valid at a later time,
/// consider scheduling it for later processing in such case.
pub fn on_attestation(
    store: &mut Store,
    attestation: Attestation,
    is_from_block: bool,
) -> anyhow::Result<()> {
    store.validate_on_attestation(&attestation, is_from_block)?;

    store.store_target_checkpoint_state(attestation.data.target)?;

    // Get state at the `target` to fully validate attestation
    let target_state = &store.checkpoint_states[&attestation.data.target];
    let indexed_attestation = target_state.get_indexed_attestation(&attestation)?;
    ensure!(target_state.is_valid_indexed_attestation(&indexed_attestation)?);

    // Update latest messages for attesting indices
    store.update_latest_messages(indexed_attestation.attesting_indices.to_vec(), attestation)?;

    Ok(())
}
