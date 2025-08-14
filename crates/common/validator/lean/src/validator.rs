use ream_chain_lean::slot::get_current_slot;
use ream_consensus_lean::state::LeanState;

pub fn is_proposer(state: &LeanState, validator_index: u64) -> anyhow::Result<bool> {
    Ok(get_current_slot() % state.config.num_validators == validator_index)
}
