use crate::{
    constants::{MIN_VALIDATOR_WITHDRAWABILITY_DELAY, SAFETY_DECAY},
    electra::beacon_state::BeaconState,
};

/// Returns the weak subjectivity period for the current ``state``.
/// This computation takes into account the effect of:
/// - validator set churn (bounded by ``get_balance_churn_limit()`` per epoch).
pub fn compute_weak_subjectivity_period(state: &BeaconState) -> u64 {
    let active_balance_eth = state.get_total_active_balance();
    let delta = state.get_balance_churn_limit();
    let epochs_for_validator_set_churn = SAFETY_DECAY * active_balance_eth / (2 * delta * 100);
    MIN_VALIDATOR_WITHDRAWABILITY_DELAY + epochs_for_validator_set_churn
}
