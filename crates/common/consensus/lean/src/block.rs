use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use ream_metrics::{PQ_SIGNATURE_ATTESTATION_VERIFICATION_TIME, start_timer, stop_timer};
use ream_post_quantum_crypto::leansig::signature::Signature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

#[cfg(feature = "devnet1")]
use crate::attestation::Attestation;
#[cfg(feature = "devnet2")]
use crate::attestation::{AggregatedAttestation, AggregatedAttestations};
use crate::state::LeanState;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockSignatures {
    pub attestation_signatures: VariableList<Signature, U4096>,
    pub proposer_signature: Signature,
}

/// Envelope carrying a block, an attestation from proposer, and aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SignedBlockWithAttestation {
    pub message: BlockWithAttestation,
    #[cfg(feature = "devnet2")]
    pub signature: BlockSignatures,
    #[cfg(feature = "devnet1")]
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
        #[cfg(feature = "devnet1")]
        let mut all_attestations = block.body.attestations.to_vec();
        #[cfg(feature = "devnet2")]
        let aggregated_attestations = &block.body.attestations;
        #[cfg(feature = "devnet2")]
        let attestation_signatures = &signatures.attestation_signatures;

        #[cfg(feature = "devnet1")]
        all_attestations.push(self.message.proposer_attestation.clone());

        #[cfg(feature = "devnet1")]
        ensure!(
            signatures.len() == all_attestations.len(),
            "Number of signatures {} does not match number of attestations {}",
            signatures.len(),
            all_attestations.len(),
        );
        #[cfg(feature = "devnet2")]
        ensure!(
            attestation_signatures.len() == aggregated_attestations.len(),
            "Number of signatures {} does not match number of attestations {}",
            attestation_signatures.len(),
            aggregated_attestations.len(),
        );

        let validators = &parent_state.validators;

        #[cfg(feature = "devnet1")]
        for (attestation, signature) in all_attestations.iter().zip(signatures.iter()) {
            ensure!(
                attestation.validator_id < validators.len() as u64,
                "Validator index out of range"
            );
            let validator = validators
                .get(attestation.validator_id as usize)
                .ok_or(anyhow!("Failed to get validator"))?;

            if verify_signatures {
                let timer = start_timer(&PQ_SIGNATURE_ATTESTATION_VERIFICATION_TIME, &[]);
                ensure!(
                    signature.verify(
                        &validator.public_key,
                        attestation.data.slot as u32,
                        &attestation.tree_hash_root(),
                    )?,
                    "Failed to verify"
                );
                stop_timer(timer);
            }
        }

        #[cfg(feature = "devnet2")]
        {
            let mut signature_iter = attestation_signatures.iter();
            for aggregated_attestation in aggregated_attestations.iter() {
                let validator_ids: Vec<usize> = aggregated_attestation
                    .aggregation_bits
                    .iter()
                    .enumerate()
                    .filter(|(_, bit)| *bit)
                    .map(|(index, _)| index)
                    .collect();

                let attestation_root = aggregated_attestation.message.tree_hash_root();

                for validator_id in validator_ids {
                    let signature = signature_iter.next().ok_or_else(|| {
                        anyhow!("Missing signature for validator index {validator_id}")
                    })?;

                    ensure!(
                        validator_id < validators.len(),
                        "Validator index out of range"
                    );

                    let validator = validators
                        .get(validator_id)
                        .ok_or_else(|| anyhow!("Failed to get validator"))?;

                    if verify_signatures {
                        let timer = start_timer(&PQ_SIGNATURE_ATTESTATION_VERIFICATION_TIME, &[]);
                        ensure!(
                            signature.verify(
                                &validator.public_key,
                                aggregated_attestation.message.slot as u32,
                                &attestation_root,
                            )?,
                            "Attestation signature verification failed"
                        );
                        stop_timer(timer);
                    }
                }
            }

            let proposer_attestation = &self.message.proposer_attestation;
            let proposer_signature = &signatures.proposer_signature;

            ensure!(
                proposer_attestation.validator_id < validators.len() as u64,
                "Proposer index out of range"
            );

            let proposer = validators
                .get(proposer_attestation.validator_id as usize)
                .ok_or_else(|| anyhow!("Failed to get proposer validator"))?;

            if verify_signatures {
                ensure!(
                    proposer_signature.verify(
                        &proposer.public_key,
                        proposer_attestation.data.slot as u32,
                        &proposer_attestation.data.tree_hash_root(),
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
    #[cfg(feature = "devnet1")]
    pub proposer_attestation: Attestation,
    #[cfg(feature = "devnet2")]
    pub proposer_attestation: AggregatedAttestations,
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
    #[cfg(feature = "devnet1")]
    pub attestations: VariableList<Attestation, U4096>,
    #[cfg(feature = "devnet2")]
    pub attestations: VariableList<AggregatedAttestation, U4096>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlockWithSignatures {
    pub block: Block,
    pub signatures: VariableList<Signature, U4096>,
}

#[cfg(test)]
#[cfg(feature = "devnet1")]
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
