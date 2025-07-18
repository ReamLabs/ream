use alloy_primitives::B256;
use anyhow::ensure;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::beacon_block_body::BeaconBlockBody;
use crate::{
    beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader},
    blob_sidecar::BlobSidecar,
    electra::{
        blinded_beacon_block::{BlindedBeaconBlock, SignedBlindedBeaconBlock},
        blinded_beacon_block_body::BlindedBeaconBlockBody,
    },
    execution_engine::rpc_types::get_blobs::{Blob, BlobAndProofV1},
    polynomial_commitments::kzg_proof::KZGProof,
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
#[cfg_attr(feature = "test_consensus", derive(TreeHash))]
pub struct SignedBeaconBlock {
    pub message: BeaconBlock,
    pub signature: BLSSignature,
}

impl SignedBeaconBlock {
    pub fn signed_header(&self) -> SignedBeaconBlockHeader {
        SignedBeaconBlockHeader {
            message: BeaconBlockHeader {
                slot: self.message.slot,
                proposer_index: self.message.proposer_index,
                parent_root: self.message.parent_root,
                state_root: self.message.state_root,
                body_root: self.message.body.tree_hash_root(),
            },
            signature: self.signature.clone(),
        }
    }

    pub fn blob_sidecar(
        &self,
        blob_and_proof: BlobAndProofV1,
        index: u64,
    ) -> anyhow::Result<BlobSidecar> {
        ensure!(
            index < self.message.body.blob_kzg_commitments.len() as u64,
            "index must be less than the number of blob kzg commitments"
        );
        Ok(BlobSidecar {
            index,
            blob: blob_and_proof.blob,
            kzg_commitment: self.message.body.blob_kzg_commitments[index as usize],
            kzg_proof: blob_and_proof.proof,
            signed_block_header: self.signed_header(),
            kzg_commitment_inclusion_proof: self
                .message
                .body
                .blob_kzg_commitment_inclusion_proof(index)?
                .into(),
        })
    }

    pub fn get_blob_sidecars(
        &self,
        blobs: Vec<Blob>,
        blob_kzg_proofs: Vec<KZGProof>,
    ) -> anyhow::Result<Vec<BlobSidecar>> {
        blobs
            .into_iter()
            .zip(blob_kzg_proofs)
            .enumerate()
            .map(|(index, (blob, proof))| {
                self.blob_sidecar(BlobAndProofV1 { blob, proof }, index as u64)
            })
            .collect::<anyhow::Result<Vec<_>>>()
    }

    pub fn as_signed_blinded_beacon_block(&self) -> SignedBlindedBeaconBlock {
        SignedBlindedBeaconBlock {
            message: BlindedBeaconBlock {
                slot: self.message.slot,
                proposer_index: self.message.proposer_index,
                parent_root: self.message.parent_root,
                state_root: self.message.state_root,
                body: BlindedBeaconBlockBody {
                    randao_reveal: self.message.body.randao_reveal.clone(),
                    eth1_data: self.message.body.eth1_data.clone(),
                    graffiti: self.message.body.graffiti,
                    proposer_slashings: self.message.body.proposer_slashings.clone(),
                    attester_slashings: self.message.body.attester_slashings.clone(),
                    attestations: self.message.body.attestations.clone(),
                    deposits: self.message.body.deposits.clone(),
                    voluntary_exits: self.message.body.voluntary_exits.clone(),
                    sync_aggregate: self.message.body.sync_aggregate.clone(),
                    execution_payload_header: self
                        .message
                        .body
                        .execution_payload
                        .to_execution_payload_header(),
                    bls_to_execution_changes: self.message.body.bls_to_execution_changes.clone(),
                    blob_kzg_commitments: self.message.body.blob_kzg_commitments.clone(),
                    execution_requests: self.message.body.execution_requests.clone(),
                },
            },
            signature: self.signature.clone(),
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Default,
)]
pub struct BeaconBlock {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BeaconBlockBody,
}

impl BeaconBlock {
    pub fn block_root(&self) -> B256 {
        self.tree_hash_root()
    }
}
