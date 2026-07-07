use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use ream_metrics::{
    PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL, PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL,
    PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME, inc_int_counter_vec, start_timer, stop_timer,
};
#[cfg(feature = "devnet4")]
use ream_metrics::{
    PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL, PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL,
};
#[cfg(feature = "devnet4")]
use ream_post_quantum_crypto::lean_multisig::aggregate::verify_aggregate_signature;
#[cfg(feature = "devnet5")]
use ream_post_quantum_crypto::lean_multisig::type_2::type_2_verify_block;
#[cfg(feature = "devnet4")]
use ream_post_quantum_crypto::leansig::signature::Signature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

#[cfg(feature = "devnet4")]
use crate::attestation::{AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof};
#[cfg(feature = "devnet5")]
use crate::attestation::{
    AggregatedAttestation, AggregatedAttestations, MultiMessageAggregate, SingleMessageAggregate,
};
use crate::state::LeanState;

#[cfg(feature = "devnet4")]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockSignatures {
    pub attestation_signatures: VariableList<AggregatedSignatureProof, U4096>,
    pub proposer_signature: Signature,
}

/// Envelope carrying a block, an attestation from proposer, and aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SignedBlock {
    pub block: Block,
    #[cfg(feature = "devnet4")]
    pub signature: BlockSignatures,
    #[cfg(feature = "devnet5")]
    pub proof: MultiMessageAggregate,
}

impl SignedBlock {
    #[cfg(feature = "devnet4")]
    pub fn verify_signatures(
        &self,
        parent_state: &LeanState,
        verify_signatures: bool,
    ) -> anyhow::Result<bool> {
        let block = &self.block;
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

            // Collect attestation public keys for all validators in this aggregation
            let public_keys: Vec<_> = validator_ids
                .iter()
                .map(|&validator_id| {
                    validators
                        .get(validator_id)
                        .map(|validator| validator.attestation_public_key)
                        .ok_or_else(|| anyhow!("Failed to get validator {validator_id}"))
                })
                .collect::<Result<Vec<_>, _>>()?;

            if verify_signatures {
                let timer = start_timer(&PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME, &[]);

                match verify_aggregate_signature(
                    &public_keys,
                    &attestation_root,
                    #[cfg(feature = "devnet4")]
                    aggregated_signature.proof_data.as_ref(),
                    #[cfg(feature = "devnet5")]
                    aggregated_signature.proof.as_ref(),
                    aggregated_attestation.message.slot as u32,
                ) {
                    Ok(()) => {
                        stop_timer(timer);
                        inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL, &[]);
                        for _ in &validator_ids {
                            inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_VALID_TOTAL, &[]);
                        }
                    }
                    Err(err) => {
                        stop_timer(timer);
                        inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL, &[]);
                        for _ in &validator_ids {
                            inc_int_counter_vec(&PQ_SIG_ATTESTATION_SIGNATURES_INVALID_TOTAL, &[]);
                        }
                        return Err(anyhow!(
                            "Attestation aggregated signature verification failed: {err}"
                        ));
                    }
                }
            }
        }

        let proposer_index = block.proposer_index;
        ensure!(
            proposer_index < validators.len() as u64,
            "Proposer index out of range"
        );

        let proposer = validators
            .get(proposer_index as usize)
            .ok_or_else(|| anyhow!("Failed to get proposer validator"))?;

        if verify_signatures {
            ensure!(
                signatures.proposer_signature.verify(
                    &proposer.proposal_public_key,
                    block.slot as u32,
                    &block.tree_hash_root(),
                )?,
                "Proposer block signature verification failed"
            );
        }

        Ok(true)
    }

    #[cfg(feature = "devnet5")]
    pub fn verify_signatures(
        &self,
        parent_state: &LeanState,
        verify_signatures: bool,
    ) -> anyhow::Result<bool> {
        let block = &self.block;
        let aggregated_attestations = &block.body.attestations;
        let validators = &parent_state.validators;

        let mut public_keys_per_component: Vec<Vec<_>> =
            Vec::with_capacity(aggregated_attestations.len() + 1);
        let mut expected_bindings: Vec<([u8; 32], u32)> =
            Vec::with_capacity(aggregated_attestations.len() + 1);

        for aggregated_attestation in aggregated_attestations.iter() {
            let validator_ids: Vec<usize> = aggregated_attestation
                .aggregation_bits
                .iter()
                .enumerate()
                .filter(|(_, bit)| *bit)
                .map(|(index, _)| index)
                .collect();

            let public_keys: Vec<_> = validator_ids
                .iter()
                .map(|&validator_id| {
                    validators
                        .get(validator_id)
                        .map(|validator| validator.attestation_public_key)
                        .ok_or_else(|| anyhow!("Failed to get validator {validator_id}"))
                })
                .collect::<Result<Vec<_>, _>>()?;

            public_keys_per_component.push(public_keys);
            expected_bindings.push((
                aggregated_attestation.message.tree_hash_root().into(),
                aggregated_attestation.message.slot as u32,
            ));
        }

        let proposer_index = block.proposer_index;
        ensure!(
            proposer_index < validators.len() as u64,
            "Proposer index out of range"
        );
        let proposer = validators
            .get(proposer_index as usize)
            .ok_or_else(|| anyhow!("Failed to get proposer validator"))?;

        public_keys_per_component.push(vec![proposer.proposal_public_key]);
        expected_bindings.push((block.tree_hash_root().into(), block.slot as u32));

        if verify_signatures {
            let timer = start_timer(&PQ_SIG_AGGREGATED_SIGNATURES_VERIFICATION_TIME, &[]);
            match type_2_verify_block(
                self.proof.as_ref(),
                &public_keys_per_component,
                &expected_bindings,
            ) {
                Ok(()) => {
                    stop_timer(timer);
                    inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_VALID_TOTAL, &[]);
                }
                Err(err) => {
                    stop_timer(timer);
                    inc_int_counter_vec(&PQ_SIG_AGGREGATED_SIGNATURES_INVALID_TOTAL, &[]);
                    return Err(anyhow!("Block proof verification failed: {err}"));
                }
            }
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
    #[cfg(feature = "reth")]
    pub execution_payload: ream_execution_rpc_types::electra::execution_payload::ExecutionPayload,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockWithSignatures {
    pub block: Block,

    #[cfg(feature = "devnet4")]
    pub signatures: VariableList<AggregatedSignatureProof, U4096>,

    #[cfg(feature = "devnet5")]
    pub signatures: VariableList<SingleMessageAggregate, U4096>,

    #[cfg(feature = "devnet5")]
    pub attestation_public_keys: Vec<Vec<ream_post_quantum_crypto::leansig::public_key::PublicKey>>,
}
