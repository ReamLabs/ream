use anyhow::ensure;
use ream_consensus::{
    checkpoint::Checkpoint, electra::beacon_state::BeaconState, misc::compute_epoch_at_slot,
};
use ream_fork_choice::store::Store;

/// The state of the weak subjectivity verification.
#[derive(Debug)]
pub enum WeakSubjectivityState {
    /// The state is verified to be within the weak subjectivity period.
    CheckpointAlreadyVerified,
    /// The state is pending verification.
    CheckpointPendingVerification,
    /// No weak subjectivity checkpoint was provided.
    None,
}

/// Check whether the `state` recovered from the `weak_subjectivity_checkpoint` is not stale.
pub fn is_within_weak_subjectivity_period(
    store: &Store,
    weak_subjectivity_state: BeaconState,
    weak_subjectivity_checkpoint: Checkpoint,
) -> anyhow::Result<bool> {
    ensure!(
        weak_subjectivity_state.latest_block_header.state_root == weak_subjectivity_checkpoint.root,
        "State root must be equal to checkpoint root"
    );
    ensure!(
        compute_epoch_at_slot(weak_subjectivity_state.slot) == weak_subjectivity_checkpoint.epoch,
        "State epoch must be equal to checkpoint epoch"
    );

    let weak_subjectivity_period = weak_subjectivity_state.compute_weak_subjectivity_period();
    let weak_subjectivity_state_epoch = compute_epoch_at_slot(weak_subjectivity_state.slot);
    let current_epoch = compute_epoch_at_slot(store.get_current_slot()?);
    Ok(current_epoch <= weak_subjectivity_state_epoch + weak_subjectivity_period)
}

/// Check whether a `state` contains the Weak Subjectivity Root.
pub fn verify_state_from_weak_subjectivity_checkpoint(
    state: &BeaconState,
    weak_subjectivity_checkpoint: &Checkpoint,
) -> anyhow::Result<bool> {
    if weak_subjectivity_checkpoint.epoch < state.get_current_epoch() {
        ensure!(
            state.get_block_root(weak_subjectivity_checkpoint.epoch)?
                == weak_subjectivity_checkpoint.root,
            "Weak subjectivity checkpoint not found"
        );
        Ok(true)
    } else {
        Ok(false)
    }
}
