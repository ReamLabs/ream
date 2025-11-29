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
                index: index as u64,
            })
            .collect::<Vec<_>>();

        let (block_1, _) = setup_genesis(1000, public_keys_1.clone());
        let (block_1_copy, _) = setup_genesis(1000, public_keys_1.clone());
        assert_eq!(block_1.tree_hash_root(), block_1_copy.tree_hash_root());

        let public_keys_2 = (0..3)
            .map(|index| Validator {
                public_key: PublicKey::new(FixedBytes::from_slice(&[index + 10; 52])),
                index: index as u64,
            })
            .collect::<Vec<_>>();

        let (block_2, _) = setup_genesis(1000, public_keys_2.clone());
        assert_ne!(block_1.tree_hash_root(), block_2.tree_hash_root());

        let (block_3, _) = setup_genesis(2000, public_keys_1.clone());
        assert_ne!(block_1.tree_hash_root(), block_3.tree_hash_root());

        assert_eq!(
            block_1.tree_hash_root().encode_hex(),
            "cc03f11dd80dd79a4add86265fad0a141d0a553812d43b8f2c03aa43e4b002e3"
        );
        assert_eq!(
            block_2.tree_hash_root().encode_hex(),
            "6bd5347aa1397c63ed8558079fdd3042112a5f4258066e3a659a659ff75ba14f"
        );
        assert_eq!(
            block_3.tree_hash_root().encode_hex(),
            "ce48a709189aa2b23b6858800996176dc13eb49c0c95d717c39e60042de1ac91"
        );
    }
}
