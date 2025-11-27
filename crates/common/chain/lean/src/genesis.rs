use alloy_primitives::B256;
use ream_consensus_lean::{
    block::{Block, BlockBody},
    state::LeanState,
    validator::Validator,
};
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
pub fn setup_genesis(genesis_time: u64, validators: Vec<Validator>) -> (Block, LeanState) {
    let genesis_state = LeanState::generate_genesis(genesis_time, Some(validators));
    let genesis_block = genesis_block(genesis_state.tree_hash_root());

    (genesis_block, genesis_state)
}

#[cfg(test)]
mod test {
    use alloy_primitives::{FixedBytes, hex::ToHexExt};
    use ream_consensus_lean::validator::Validator;
    use ream_post_quantum_crypto::leansig::public_key::PublicKey;
    use tree_hash::TreeHash;

    use crate::genesis::setup_genesis;

    #[test]
    fn test_genesis_block_hash_comparison() {
        let public_keys_1 = (0..3)
            .map(|index| Validator {
                public_key: PublicKey::new(FixedBytes::from_slice(&[index + 1; 52])),
                index: 0,
            })
            .collect::<Vec<_>>();

        let (block_1, _) = setup_genesis(1000, public_keys_1.clone());
        let (block_1_copy, _) = setup_genesis(1000, public_keys_1.clone());
        assert_eq!(block_1.tree_hash_root(), block_1_copy.tree_hash_root());

        let public_keys_2 = (0..3)
            .map(|index| Validator {
                public_key: PublicKey::new(FixedBytes::from_slice(&[index + 10; 52])),
                index: 0,
            })
            .collect::<Vec<_>>();

        let (block_2, _) = setup_genesis(1000, public_keys_2.clone());
        assert_ne!(block_1.tree_hash_root(), block_2.tree_hash_root());

        let (block_3, _) = setup_genesis(2000, public_keys_1.clone());
        assert_ne!(block_1.tree_hash_root(), block_3.tree_hash_root());

        assert_eq!(
            block_1.tree_hash_root().encode_hex(),
            "4c0bcc4750b71818224a826cd59f8bcb75ae2920eb3e75b4097b818be6d1049a"
        );
        assert_eq!(
            block_2.tree_hash_root().encode_hex(),
            "639b6162e6b432653a77a64b678717e7634428eda88ad6ccb1862e6397c0c47b"
        );
        assert_eq!(
            block_3.tree_hash_root().encode_hex(),
            "6593976e31c915b5d534e2ee6172652aed7690be24777947de39c726aa2af59e"
        );
    }
}
