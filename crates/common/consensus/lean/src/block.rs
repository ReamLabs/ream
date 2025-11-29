use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use ream_post_quantum_crypto::leansig::signature::Signature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::{attestation::Attestation, state::LeanState};

/// Envelope carrying a block, an attestation from proposer, and aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SignedBlockWithAttestation {
    pub message: BlockWithAttestation,
    pub signature: VariableList<Signature, U4096>,
}

impl SignedBlockWithAttestation {
    pub fn verify_signatures(
        &self,
        parent_state: &LeanState,
        verify_signatures: bool,
    ) -> anyhow::Result<bool> {
        let block = &self.message.block;
        let signatures = &self.signature;
        let mut all_attestations = block.body.attestations.to_vec();

        all_attestations.push(self.message.proposer_attestation.clone());

        ensure!(
            signatures.len() == all_attestations.len(),
            "Number of signatures {} does not match number of attestations {}",
            signatures.len(),
            all_attestations.len(),
        );
        let validators = &parent_state.validators;

        for (attestation, signature) in all_attestations.iter().zip(signatures.iter()) {
            let validator_id = attestation.validator_id as usize;
            ensure!(
                validator_id < validators.len(),
                "Validator index out of range"
            );
            let validator = validators
                .get(validator_id)
                .ok_or(anyhow!("Failed to get validator"))?;

            if verify_signatures {
                ensure!(
                    signature.verify(
                        &validator.public_key,
                        attestation.data.slot as u32,
                        &attestation.tree_hash_root(),
                    )?,
                    "Failed to verify"
                );
            }
        }

        Ok(true)
    }
}

/// Bundle containing a block and the proposer's attestation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockWithAttestation {
    pub block: Block,
    pub proposer_attestation: Attestation,
}

/// Represents a block in the Lean chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Block {
    pub slot: u64,
    pub proposer_index: u64,
    // Diverged from Python implementation: Disallow `None` (uses `B256::ZERO` instead)
    pub parent_root: B256,
    // Diverged from Python implementation: Disallow `None` (uses `B256::ZERO` instead)
    pub state_root: B256,
    pub body: BlockBody,
}

/// Represents a block header in the Lean chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body_root: B256,
}

impl From<Block> for BlockHeader {
    fn from(block: Block) -> Self {
        BlockHeader {
            slot: block.slot,
            proposer_index: block.proposer_index,
            parent_root: block.parent_root,
            state_root: block.state_root,
            body_root: block.body.tree_hash_root(),
        }
    }
}

/// Represents the body of a block in the Lean chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlockBody {
    pub attestations: VariableList<Attestation, U4096>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlockWithSignatures {
    pub block: Block,
    pub signatures: VariableList<Signature, U4096>,
}

#[cfg(test)]
mod tests {

    use alloy_primitives::hex;
    use ssz::{Decode, Encode};

    use super::*;
    use crate::{attestation::AttestationData, checkpoint::Checkpoint};

    #[test]
    fn test_encode_decode_signed_block_with_attestation_roundtrip() -> anyhow::Result<()> {
        let signed_block_with_attestation = SignedBlockWithAttestation {
            message: BlockWithAttestation {
                block: Block {
                    slot: 0,
                    proposer_index: 0,
                    parent_root: B256::ZERO,
                    state_root: B256::ZERO,
                    body: BlockBody {
                        attestations: Default::default(),
                    },
                },
                proposer_attestation: Attestation {
                    validator_id: 0,
                    data: AttestationData {
                        slot: 0,
                        head: Checkpoint::default(),
                        target: Checkpoint::default(),
                        source: Checkpoint::default(),
                    },
                },
            },
            signature: VariableList::default(),
        };

        let encode = signed_block_with_attestation.as_ssz_bytes();
        let decoded = SignedBlockWithAttestation::from_ssz_bytes(&encode);
        assert_eq!(
            hex::encode(encode),
            "08000000ec0000008c0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005400000004000000"
        );
        assert_eq!(decoded, Ok(signed_block_with_attestation));

        Ok(())
    }
}
