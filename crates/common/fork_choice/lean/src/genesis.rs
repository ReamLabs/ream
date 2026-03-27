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
pub fn setup_genesis(genesis_time: u64, validators: Vec<Validator>) -> (Block, LeanState) {
    let genesis_state = LeanState::generate_genesis(genesis_time, Some(validators));
    let genesis_block = genesis_block(genesis_state.tree_hash_root());

    (genesis_block, genesis_state)
}

#[cfg(test)]
mod test {
    use alloy_primitives::FixedBytes;
    #[cfg(feature = "devnet3")]
    use alloy_primitives::hex::ToHexExt;
    use ream_consensus_lean::validator::Validator;
    use ream_post_quantum_crypto::leansig::public_key::PublicKey;
    use tree_hash::TreeHash;

    use crate::genesis::setup_genesis;

    fn make_test_validator(index: u8) -> Validator {
        #[cfg(feature = "devnet3")]
        {
            Validator {
                public_key: PublicKey::new(FixedBytes::from_slice(&[index; 52])),
                index: (index - 1) as u64,
            }
        }
        #[cfg(feature = "devnet4")]
        {
            Validator {
                attestation_public_key: PublicKey::new(FixedBytes::from_slice(&[index; 52])),
                proposal_public_key: PublicKey::new(FixedBytes::from_slice(&[index; 52])),
                index: (index - 1) as u64,
            }
        }
    }

    #[test]
    fn test_genesis_block_hash_comparison() {
        let public_keys_1 = (0..3)
            .map(|index| make_test_validator(index + 1))
            .collect::<Vec<_>>();

        let (block_1, _) = setup_genesis(1000, public_keys_1.clone());
        let (block_1_copy, _) = setup_genesis(1000, public_keys_1.clone());
        assert_eq!(block_1.tree_hash_root(), block_1_copy.tree_hash_root());

        let public_keys_2 = (0..3)
            .map(|index| make_test_validator(index + 10))
            .collect::<Vec<_>>();

        let (block_2, _) = setup_genesis(1000, public_keys_2.clone());
        assert_ne!(block_1.tree_hash_root(), block_2.tree_hash_root());

        let (block_3, _) = setup_genesis(2000, public_keys_1.clone());
        assert_ne!(block_1.tree_hash_root(), block_3.tree_hash_root());

        // Hash values differ between devnet3 and devnet4 due to different Validator struct layout
        #[cfg(feature = "devnet3")]
        {
            assert_eq!(
                block_1.tree_hash_root().encode_hex(),
                "cc03f11dd80dd79a4add86265fad0a141d0a553812d43b8f2c03aa43e4b002e3"
            );
            assert_eq!(
                block_2.tree_hash_root().encode_hex(),
                "cad1ca340fd23738541ee49ded6e28aa422e6328af56e7445c1a7cd1bf83f2ee"
            );
            assert_eq!(
                block_3.tree_hash_root().encode_hex(),
                "ce48a709189aa2b23b6858800996176dc13eb49c0c95d717c39e60042de1ac91"
            );
        }
    }
}
