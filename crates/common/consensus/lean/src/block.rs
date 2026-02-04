use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use ream_metrics::{PQ_SIGNATURE_ATTESTATION_VERIFICATION_TIME, start_timer, stop_timer};
use ream_post_quantum_crypto::{
    lean_multisig::aggregate::verify_aggregate_signature, leansig::signature::Signature,
};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::{
    attestation::{AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof},
    state::LeanState,
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockSignatures {
    pub attestation_signatures: VariableList<AggregatedSignatureProof, U4096>,
    pub proposer_signature: Signature,
}

/// Envelope carrying a block, an attestation from proposer, and aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SignedBlockWithAttestation {
    pub message: BlockWithAttestation,
    pub signature: BlockSignatures,
}

impl SignedBlockWithAttestation {
    pub fn verify_signatures(
        &self,
        parent_state: &LeanState,
        verify_signatures: bool,
    ) -> anyhow::Result<bool> {
        let block = &self.message.block;
        let signatures = &self.signature;
        let aggregated_attestations = &block.body.attestations;
        let attestation_signatures = &signatures.attestation_signatures;

        ensure!(
            attestation_signatures.len() == aggregated_attestations.len(),
            "Number of signatures {} does not match number of attestations {}",
            attestation_signatures.len(),
            aggregated_attestations.len(),
        );

        let validators = &parent_state.validators;

        for (aggregated_attestation, aggregated_signature) in aggregated_attestations
            .iter()
            .zip(attestation_signatures.iter())
        {
            let validator_ids: Vec<usize> = aggregated_attestation
                .aggregation_bits
                .iter()
                .enumerate()
                .filter(|(_, bit)| *bit)
                .map(|(index, _)| index)
                .collect();

            let attestation_root = aggregated_attestation.message.tree_hash_root();

            // Validate all validator indices are in range
            for &validator_id in &validator_ids {
                ensure!(
                    validator_id < validators.len(),
                    "Validator index out of range"
                );
            }

            // Collect public keys for all validators in this aggregation
            let public_keys: Vec<_> = validator_ids
                .iter()
                .map(|&validator_id| {
                    validators
                        .get(validator_id)
                        .map(|validator| validator.public_key)
                        .ok_or_else(|| anyhow!("Failed to get validator {validator_id}"))
                })
                .collect::<Result<Vec<_>, _>>()?;

            if verify_signatures {
                let timer = start_timer(&PQ_SIGNATURE_ATTESTATION_VERIFICATION_TIME, &[]);

                verify_aggregate_signature(
                    &public_keys,
                    &attestation_root,
                    aggregated_signature.proof_data.as_ref(),
                    aggregated_attestation.message.slot as u32,
                )
                .map_err(|err| {
                    anyhow!("Attestation aggregated signature verification failed: {err}")
                })?;
                stop_timer(timer);
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
                "Proposer signature verification failed"
            );
        }

        Ok(true)
    }
}

/// Bundle containing a block and the proposer's attestation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockWithAttestation {
    pub block: Block,
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
    pub attestations: VariableList<AggregatedAttestation, U4096>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockWithSignatures {
    pub block: Block,
    pub signatures: VariableList<AggregatedSignatureProof, U4096>,
}
