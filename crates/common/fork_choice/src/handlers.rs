use alloy_primitives::{B256, map::HashSet};
use anyhow::{bail, ensure};
use ream_consensus::{
    attester_slashing::AttesterSlashing,
    checkpoint::Checkpoint,
    constants::{INTERVALS_PER_SLOT, SECONDS_PER_SLOT},
    deneb::beacon_block::SignedBeaconBlock,
    execution_engine::{blob_versioned_hashes::blob_versioned_hashes, engine_trait::ExecutionApi},
    kzg_commitment::KZGCommitment,
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch},
    predicates::is_slashable_attestation_data,
};
use ream_polynomial_commitments::handlers::verify_blob_kzg_proof_batch;
use tree_hash::TreeHash;

use crate::store::Store;

pub async fn is_data_available(
    beacon_block_root: B256,
    store: &Store,
    blob_kzg_commitments: &[KZGCommitment],
    execution_engine: &impl ExecutionApi,
) -> anyhow::Result<bool> {
    // `retrieve_blobs_and_proofs` is implementation and context dependent
    // It returns all the blobs for the given block root, and raises an exception if not available
    // Note: the p2p network does not guarantee sidecar retrieval outside of
    // `MIN_EPOCHS_FOR_BLOB_SIDECARS_REQUESTS`

    let Some(beacon_block) = store.blocks.get(&beacon_block_root) else {
        bail!("could not get beack block");
    };
    let blob_versioned_hashes = blob_versioned_hashes(&beacon_block.body.execution_payload)?;
    let blobs_and_proofs = execution_engine
        .engine_get_blobs_v1(blob_versioned_hashes)
        .await?;

    let mut blobs = vec![];
    let mut proofs = vec![];

    for block_and_proof in blobs_and_proofs {
        let block_and_proof =
            block_and_proof.ok_or_else(|| anyhow::anyhow!("Invalid proposer index"))?;
        blobs.push(block_and_proof.blob);
        proofs.push(block_and_proof.proof);
    }

    verify_blob_kzg_proof_batch(&blobs, blob_kzg_commitments, &proofs)?;
    Ok(true)
}

pub fn get_ancestor(store: &Store, root: B256, slot: u64) -> B256 {
    if let Some(block) = store.blocks.get(&root) {
        if block.slot > slot {
            return get_ancestor(store, block.parent_root, slot);
        }
    }
    root
}

/// Compute the checkpoint block for epoch ``epoch`` in the chain of block ``root``
pub fn get_checkpoint_block(store: &Store, root: B256, epoch: u64) -> B256 {
    let epoch_first_slot = compute_start_slot_at_epoch(epoch);
    get_ancestor(store, root, epoch_first_slot)
}

/// Update checkpoints in store if necessary
pub fn update_checkpoints(
    store: &mut Store,
    justified_checkpoint: Checkpoint,
    finalized_checkpoint: Checkpoint,
) {
    // Update justified checkpoint
    if justified_checkpoint.epoch > store.justified_checkpoint.epoch {
        store.justified_checkpoint = justified_checkpoint
    }
    // Update finalized checkpoint
    if finalized_checkpoint.epoch > store.finalized_checkpoint.epoch {
        store.finalized_checkpoint = finalized_checkpoint
    }
}

/// Update unrealized checkpoints in store if necessary
pub fn update_unrealized_checkpoints(
    store: &mut Store,
    unrealized_justified_checkpoint: Checkpoint,
    unrealized_finalized_checkpoint: Checkpoint,
) {
    // Update unrealized justified checkpoint
    if unrealized_justified_checkpoint.epoch > store.unrealized_justified_checkpoint.epoch {
        store.unrealized_justified_checkpoint = unrealized_justified_checkpoint
    }
    // Update unrealized finalized checkpoint
    if unrealized_finalized_checkpoint.epoch > store.unrealized_finalized_checkpoint.epoch {
        store.unrealized_finalized_checkpoint = unrealized_finalized_checkpoint
    }
}

pub fn get_current_store_epoch(store: &Store) -> u64 {
    compute_epoch_at_slot(store.get_current_slot())
}

pub fn compute_pulled_up_tip(store: &mut Store, block_root: B256) -> anyhow::Result<()> {
    let mut state = store.block_states[&block_root].clone();
    // Pull up the post-state of the block to the next epoch boundary
    state.process_justification_and_finalization()?;

    store
        .unrealized_justifications
        .insert(block_root, state.current_justified_checkpoint);
    update_unrealized_checkpoints(
        store,
        state.current_justified_checkpoint,
        state.finalized_checkpoint,
    );

    // If the block is from a prior epoch, apply the realized values
    let block_epoch = compute_epoch_at_slot(store.blocks[&block_root].slot);
    let current_epoch = get_current_store_epoch(store);
    if block_epoch < current_epoch {
        store.update_checkpoints(
            state.current_justified_checkpoint,
            state.finalized_checkpoint,
        );
    }

    Ok(())
}

/// Run ``on_block`` upon receiving a new block.
pub async fn on_block(
    store: &mut Store,
    signed_block: &SignedBeaconBlock,
    execution_engine: &impl ExecutionApi,
) -> anyhow::Result<()> {
    let block = &signed_block.message;

    // Parent block must be known
    ensure!(store.block_states.contains_key(&block.parent_root));
    // Blocks cannot be in the future. If they are, their consideration must be delayed until they
    // are in the past.
    ensure!(store.get_current_slot() >= block.slot);

    // Check that block is later than the finalized epoch slot (optimization to reduce calls to
    // get_ancestor)
    let finalized_slot = compute_start_slot_at_epoch(store.finalized_checkpoint.epoch);
    ensure!(block.slot > finalized_slot);

    // Check block is a descendant of the finalized block at the checkpoint finalized slot
    let finalized_checkpoint_block =
        get_checkpoint_block(store, block.parent_root, store.finalized_checkpoint.epoch);

    ensure!(store.finalized_checkpoint.root == finalized_checkpoint_block);

    // [New in Deneb:EIP4844]
    // Check if blob data is available
    // If not, this block MAY be queued and subsequently considered when blob data becomes available
    // *Note*: Extraneous or invalid Blobs (in addition to the expected/referenced valid blobs)
    // received on the p2p network MUST NOT invalidate a block that is otherwise valid and available
    ensure!(
        is_data_available(
            block.tree_hash_root(),
            store,
            &block.body.blob_kzg_commitments,
            execution_engine
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
    let is_first_block = store.proposer_boost_root == block_root;
    if is_timely && is_first_block {
        store.proposer_boost_root = block.tree_hash_root()
    }

    // Update checkpoints in store if necessary
    update_checkpoints(
        store,
        state.current_justified_checkpoint,
        state.finalized_checkpoint,
    );

    // Eagerly compute unrealized justification and finality.
    compute_pulled_up_tip(store, block_root)?;

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

pub fn compute_slots_since_epoch_start(slot: u64) -> u64 {
    slot - compute_start_slot_at_epoch(compute_epoch_at_slot(slot))
}

pub fn on_tick_per_slot(store: &mut Store, time: u64) -> anyhow::Result<()> {
    let previous_slot = store.get_current_slot();

    // Update store time
    store.time = time;

    let current_slot = store.get_current_slot();

    // If this is a new slot, reset store.proposer_boost_root
    if current_slot > previous_slot {
        store.proposer_boost_root = B256::ZERO;
    }

    // If a new epoch, pull-up justification and finalization from previous epoch
    if current_slot > previous_slot && compute_slots_since_epoch_start(current_slot) == 0 {
        store.update_checkpoints(
            store.unrealized_justified_checkpoint,
            store.unrealized_finalized_checkpoint,
        );
    }

    Ok(())
}

pub fn on_tick(store: &mut Store, time: u64) -> anyhow::Result<()> {
    // If the ``store.time`` falls behind, while loop catches up slot by slot
    // to ensure that every previous slot is processed with ``on_tick_per_slot``
    let tick_slot = (time - store.genesis_time) / SECONDS_PER_SLOT;
    while store.get_current_slot() < tick_slot {
        let previous_time = store.genesis_time + (store.get_current_slot() + 1) * SECONDS_PER_SLOT;
        on_tick_per_slot(store, previous_time)?;
    }

    on_tick_per_slot(store, time)?;

    Ok(())
}
