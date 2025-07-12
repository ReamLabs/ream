use alloy_primitives::B256;
use ream_bls::BLSSignature;
use ream_consensus_misc::{
    constants::{
        BLOB_KZG_COMMITMENTS_INDEX, BLOCK_BODY_MERKLE_DEPTH, EXECUTION_PAYLOAD_INDEX,
        KZG_COMMITMENTS_MERKLE_DEPTH,
    },
    eth_1_data::Eth1Data,
};
use ream_merkle::{generate_proof, merkle_tree};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    VariableList,
    typenum::{U1, U8, U16, U4096},
};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::execution_payload::ExecutionPayload;
use crate::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    bls_to_execution_change::SignedBLSToExecutionChange, deposit::Deposit,
    execution_requests::ExecutionRequests, polynomial_commitments::kzg_commitment::KZGCommitment,
    proposer_slashing::ProposerSlashing, sync_aggregate::SyncAggregate,
    voluntary_exit::SignedVoluntaryExit,
};

#[derive(
    Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Default,
)]
pub struct BeaconBlockBody {
    pub randao_reveal: BLSSignature,

    /// Eth1 data vote
    pub eth1_data: Eth1Data,

    /// Arbitrary data
    pub graffiti: B256,

    // Operations
    pub proposer_slashings: VariableList<ProposerSlashing, U16>,
    pub attester_slashings: VariableList<AttesterSlashing, U1>,
    pub attestations: VariableList<Attestation, U8>,
    pub deposits: VariableList<Deposit, U16>,
    pub voluntary_exits: VariableList<SignedVoluntaryExit, U16>,
    pub sync_aggregate: SyncAggregate,

    // Execution
    pub execution_payload: ExecutionPayload,
    pub bls_to_execution_changes: VariableList<SignedBLSToExecutionChange, U16>,
    pub blob_kzg_commitments: VariableList<KZGCommitment, U4096>,
    pub execution_requests: ExecutionRequests,
}

impl BeaconBlockBody {
    pub fn merkle_leaves(&self) -> Vec<B256> {
        vec![
            self.randao_reveal.tree_hash_root(),
            self.eth1_data.tree_hash_root(),
            self.graffiti.tree_hash_root(),
            self.proposer_slashings.tree_hash_root(),
            self.attester_slashings.tree_hash_root(),
            self.attestations.tree_hash_root(),
            self.deposits.tree_hash_root(),
            self.voluntary_exits.tree_hash_root(),
            self.sync_aggregate.tree_hash_root(),
            self.execution_payload.tree_hash_root(),
            self.bls_to_execution_changes.tree_hash_root(),
            self.blob_kzg_commitments.tree_hash_root(),
            self.execution_requests.tree_hash_root(),
        ]
    }

    pub fn data_inclusion_proof(&self, index: u64) -> anyhow::Result<Vec<B256>> {
        let tree = merkle_tree(&self.merkle_leaves(), BLOCK_BODY_MERKLE_DEPTH)?;
        generate_proof(&tree, index, BLOCK_BODY_MERKLE_DEPTH)
    }

    pub fn blob_kzg_commitment_inclusion_proof(&self, index: u64) -> anyhow::Result<Vec<B256>> {
        // inclusion proof for blob_kzg_commitment in blob_kzg_commitments
        let tree = merkle_tree(
            self.blob_kzg_commitments
                .iter()
                .map(|commitment| commitment.tree_hash_root())
                .collect::<Vec<_>>()
                .as_slice(),
            KZG_COMMITMENTS_MERKLE_DEPTH,
        )?;
        let kzg_commitment_to_kzg_commitments_proof =
            generate_proof(&tree, index, KZG_COMMITMENTS_MERKLE_DEPTH)?;

        // add branch for length of blob_kzg_commitments
        let kzg_commitments_length_root = self
            .blob_kzg_commitments
            .len()
            .to_le_bytes()
            .tree_hash_root();

        // inclusion proof for blob_kzg_commitments in beacon_block_body
        let kzg_commitments_to_block_body_proof =
            self.data_inclusion_proof(BLOB_KZG_COMMITMENTS_INDEX)?;

        // merge proofs data
        Ok([
            kzg_commitment_to_kzg_commitments_proof,
            vec![kzg_commitments_length_root],
            kzg_commitments_to_block_body_proof,
        ]
        .concat())
    }

    pub fn execution_payload_inclusion_proof(&self) -> anyhow::Result<Vec<B256>> {
        self.data_inclusion_proof(EXECUTION_PAYLOAD_INDEX)
    }
}
