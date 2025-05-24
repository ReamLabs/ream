use anyhow::ensure;
use ream_consensus::{
    checkpoint::Checkpoint, electra::beacon_state::BeaconState, misc::compute_epoch_at_slot,
    weak_subjectivity::compute_weak_subjectivity_period,
};
use ream_fork_choice::store::Store;

/// Check whether the `state` recovered from the `ws_checkpoint` is not stale.
pub fn is_within_weak_subjectivity_period(
    store: &Store,
    ws_state: BeaconState,
    ws_checkpoint: Checkpoint,
) -> anyhow::Result<bool> {
    ensure!(
        ws_state.latest_block_header.state_root == ws_checkpoint.root,
        "State root must be equal to checkpoint root"
    );
    ensure!(
        compute_epoch_at_slot(ws_state.slot) == ws_checkpoint.epoch,
        "State epoch must be equal to checkpoint epoch"
    );

    let ws_period = compute_weak_subjectivity_period(&ws_state);
    let ws_state_epoch = compute_epoch_at_slot(ws_state.slot);
    let current_epoch = compute_epoch_at_slot(store.get_current_slot()?);
    Ok(current_epoch <= ws_state_epoch + ws_period)
}

/// Check whether a `state` contains the `ws_checkpoint_root`.
pub fn verify_state_from_ws_checkpoint(
    state: &BeaconState,
    ws_checkpoint: &Checkpoint,
) -> anyhow::Result<bool> {
    Ok(state.get_block_root(ws_checkpoint.epoch)? == ws_checkpoint.root)
}
