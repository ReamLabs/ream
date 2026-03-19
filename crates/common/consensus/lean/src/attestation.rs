use std::hash::Hash;

use alloy_primitives::B256;
#[cfg(feature = "devnet4")]
use anyhow::{anyhow, ensure};
#[cfg(feature = "devnet4")]
use ream_post_quantum_crypto::{
    lean_multisig::aggregate::{
        aggregate_signatures, setup_prover, setup_verifier, verify_aggregate_signature,
    },
    leansig::public_key::PublicKey,
};
use ream_post_quantum_crypto::leansig::signature::Signature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    BitList, VariableList,
    typenum::{U4096, U1048576},
};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::checkpoint::Checkpoint;

#[cfg(feature = "devnet4")]
pub type BytecodePointOption = Option<VariableList<u8, U1048576>>;

/// Key for signature storage, combining validator ID and attestation data root.
/// Used for both gossip_signatures and aggregated_payloads maps.
#[derive(
    Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, Encode, Decode, PartialOrd, Ord,
)]
pub struct SignatureKey {
    pub validator_id: u64,
    pub data_root: B256,
}

impl SignatureKey {
    pub fn new(validator_id: u64, attestation_data: &AttestationData) -> Self {
        Self {
            validator_id,
            data_root: attestation_data.tree_hash_root(),
        }
    }

    pub fn from_parts(validator_id: u64, data_root: B256) -> Self {
        Self {
            validator_id,
            data_root,
        }
    }
}

#[cfg(feature = "devnet3")]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregatedSignatureProof {
    pub participants: BitList<U4096>,
    pub proof_data: VariableList<u8, U1048576>,
}

#[cfg(feature = "devnet3")]
impl AggregatedSignatureProof {
    pub fn new(participants: BitList<U4096>, proof_data: VariableList<u8, U1048576>) -> Self {
        Self {
            participants,
            proof_data,
        }
    }

    /// Get the validator IDs covered by this proof
    pub fn to_validator_indices(&self) -> Vec<u64> {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, bit)| *bit)
            .map(|(index, _)| index as u64)
            .collect()
    }
}

#[cfg(feature = "devnet4")]
#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct AggregatedSignatureProof {
    pub participants: BitList<U4096>,
    pub proof_data: VariableList<u8, U1048576>,
    pub bytecode_point: BytecodePointOption,
}

#[cfg(feature = "devnet4")]
impl AggregatedSignatureProof {
    pub fn new(participants: BitList<U4096>, proof_data: VariableList<u8, U1048576>) -> Self {
        Self {
            participants,
            proof_data,
            bytecode_point: None,
        }
    }

    pub fn new_recursive(
        participants: BitList<U4096>,
        proof_data: VariableList<u8, U1048576>,
        bytecode_point: VariableList<u8, U1048576>,
    ) -> Self {
        Self {
            participants,
            proof_data,
            bytecode_point: Some(bytecode_point),
        }
    }

    pub fn to_validator_indices(&self) -> Vec<u64> {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, bit)| *bit)
            .map(|(index, _)| index as u64)
            .collect()
    }
    
    pub fn aggregate(
        xmss_participants: Option<&BitList<U4096>>,
        children: &[Self],
        raw_xmss: &[(PublicKey, Signature)],
        message: &[u8; 32],
        slot: u32,
    ) -> anyhow::Result<Self> {
        if raw_xmss.is_empty() && children.is_empty() {
            return Err(anyhow!(
                "At least one raw signature or child proof is required"
            ));
        }

        if !raw_xmss.is_empty() && xmss_participants.is_none() {
            return Err(anyhow!(
                "xmss_participants is required when raw_xmss is provided"
            ));
        }

        if raw_xmss.is_empty() && children.len() < 2 {
            return Err(anyhow!(
                "At least two child proofs are required when no raw signatures are provided"
            ));
        }

        let mut aggregated_validator_ids = std::collections::HashSet::<u64>::new();

        if let Some(participants) = xmss_participants {
            aggregated_validator_ids.extend(
                participants
                    .iter()
                    .enumerate()
                    .filter(|(_, bit)| *bit)
                    .map(|(index, _)| index as u64),
            );
        }

        ensure!(
            aggregated_validator_ids.len() == raw_xmss.len(),
            "Raw signature count does not match XMSS participant count"
        );

        for child in children {
            aggregated_validator_ids.extend(child.to_validator_indices());
        }

        let participants_capacity = aggregated_validator_ids
            .iter()
            .max()
            .map_or(0, |&id| id as usize + 1);
        let mut participants = BitList::<U4096>::with_capacity(participants_capacity)
            .map_err(|err| anyhow!("BitList error: {err:?}"))?;
        for id in aggregated_validator_ids {
            participants
                .set(id as usize, true)
                .map_err(|err| anyhow!("BitList error: {err:?}"))?;
        }

        setup_prover();
        let proof_bytes = aggregate_signatures(
            &raw_xmss.iter().map(|(pk, _)| *pk).collect::<Vec<_>>(),
            &raw_xmss.iter().map(|(_, sig)| sig.clone()).collect::<Vec<_>>(),
            message,
            slot,
        )
        .map_err(|err| anyhow!("Signature aggregation failed: {err}"))?;

        Ok(Self {
            participants,
            proof_data: VariableList::new(proof_bytes)
                .map_err(|err| anyhow!("Failed to create proof_data: {err:?}"))?,
            bytecode_point: None,
        })
    }
}

#[cfg(feature = "devnet4")]
impl TreeHash for AggregatedSignatureProof {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        tree_hash::TreeHashType::Container
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        unreachable!("Struct should never be packed")
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> tree_hash::Hash256 {
        let bytecode_hash = match &self.bytecode_point {
            Some(bytes) => bytes.tree_hash_root(),
            None => tree_hash::Hash256::ZERO,
        };

        let mut leaves = Vec::with_capacity(3 * 32);
        leaves.extend_from_slice(self.participants.tree_hash_root().as_slice());
        leaves.extend_from_slice(self.proof_data.tree_hash_root().as_slice());
        leaves.extend_from_slice(bytecode_hash.as_slice());
        tree_hash::merkle_root(&leaves, 0)
    }
}

/// Attestation content describing the validator's observed chain view.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Hash)]
pub struct AttestationData {
    pub slot: u64,
    pub head: Checkpoint,
    pub target: Checkpoint,
    pub source: Checkpoint,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregatedAttestations {
    pub validator_id: u64,
    pub data: AttestationData,
}

impl AggregatedAttestation {
    /// Return the attested slot.
    pub fn slot(&self) -> u64 {
        self.message.slot
    }

    /// Return the attested head checkpoint.
    pub fn head(&self) -> Checkpoint {
        self.message.head
    }

    /// Return the attested target checkpoint.
    pub fn target(&self) -> Checkpoint {
        self.message.target
    }

    /// Return the attested source checkpoint.
    pub fn source(&self) -> Checkpoint {
        self.message.source
    }
}

/// Validator attestation bundled with its signature.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAttestation {
    pub validator_id: u64,
    pub message: AttestationData,
    /// signature over attestaion message only as it would be aggregated later in attestation
    pub signature: Signature,
}

/// Aggregated attestation consisting of participation bits and message.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregatedAttestation {
    /// U4096 = VALIDATOR_REGISTRY_LIMIT
    pub aggregation_bits: BitList<U4096>,
    pub message: AttestationData,
}

/// Aggregated attestation bundled with aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedAggregatedAttestation {
    pub data: AttestationData,
    pub proof: AggregatedSignatureProof,
}
