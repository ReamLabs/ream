use alloy_primitives::B256;
use ream_consensus_lean::{block::Block, state::LeanState};
use ream_network_spec::networks::lean_network_spec;
use tree_hash::TreeHash;

fn genesis_block(state_root: B256) -> Block {
    Block {
        state_root,
        ..Default::default()
    }
}

fn genesis_state(num_validators: u64, genesis_time: u64) -> LeanState {
    LeanState::new(num_validators, genesis_time)
}

/// Setup the genesis block and state for the Lean chain.
///
/// See lean specification:
/// <https://github.com/leanEthereum/leanSpec/blob/f869a7934fc4bccf0ba22159c64ecd398c543107/src/lean_spec/subspecs/containers/state/state.py#L65-L108>
pub fn setup_genesis() -> (Block, LeanState) {
    let (num_validators, genesis_time) = {
        let network_spec = lean_network_spec();
        (network_spec.num_validators, network_spec.genesis_time)
    };

    let genesis_state = genesis_state(num_validators, genesis_time);
    let genesis_block = genesis_block(genesis_state.tree_hash_root());

    (genesis_block, genesis_state)
}
