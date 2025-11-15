use alloy_primitives::B256;
use ream_consensus_lean::{
    block::{Block, BlockBody},
    state::LeanState,
    validator::Validator,
};
use ream_network_spec::networks::lean_network_spec;
use tree_hash::TreeHash;

fn genesis_block(state_root: B256) -> Block {
    Block {
        slot: 0,
        proposer_index: 0,
        parent_root: B256::ZERO,
        state_root,
        body: BlockBody {
            attestations: Default::default(),
        },
    }
}

/// Setup the genesis block and state for the Lean chain.
///
/// See lean specification:
/// <https://github.com/leanEthereum/leanSpec/blob/f869a7934fc4bccf0ba22159c64ecd398c543107/src/lean_spec/subspecs/containers/state/state.py#L65-L108>
pub fn setup_genesis(validators: Vec<Validator>) -> (Block, LeanState) {
    let genesis_state =
        LeanState::generate_genesis(lean_network_spec().genesis_time, Some(validators));
    let genesis_block = genesis_block(genesis_state.tree_hash_root());

    (genesis_block, genesis_state)
}
