use alloy_primitives::B256;
use ream_consensus_lean::{
    block::{Block, BlockHeader},
    state::LeanState,
};
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
/// Reference: https://github.com/ethereum/research/blob/d225a6775a9b184b5c1fd6c830cc58a375d9535f/3sf-mini/test_p2p.py#L119-L131
pub fn setup_genesis() -> (Block, LeanState) {
    let (num_validators, genesis_time) = {
        let network_spec = lean_network_spec();
        (network_spec.num_validators, network_spec.genesis_time)
    };

    let mut genesis_state = genesis_state(num_validators, genesis_time);
    let genesis_block = genesis_block(genesis_state.tree_hash_root());

    // Because of circular dependency, the `genesis_state` which is created first
    // is not aware of the genesis block. So once the genesis_block is created,
    // we need to set the `genesis_state.latest_block_header` to the genesis block.
    genesis_state.latest_block_header = BlockHeader::from(genesis_block.clone());

    (genesis_block, genesis_state)
}
