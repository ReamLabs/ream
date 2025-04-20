use alloy_consensus::Blob;
use alloy_primitives::B256;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::FixedVector;
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::beacon_block_body::BeaconBlockBody;
use crate::{
    beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader},
    blob_sidecar::BlobSidecar,
    polynomial_commitments::kzg_proof::KZGProof,
};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBeaconBlock {
    pub message: BeaconBlock,
    pub signature: BLSSignature,
}

impl SignedBeaconBlock {
    pub fn compute_signed_block_header(&self) -> SignedBeaconBlockHeader {
        let block_header = BeaconBlockHeader {
            slot: self.message.slot,
            proposer_index: self.message.proposer_index,
            parent_root: self.message.parent_root,
            state_root: self.message.state_root,
            body_root: self.message.body.tree_hash_root(),
        };
        SignedBeaconBlockHeader {
            message: block_header,
            signature: self.signature.clone(),
        }
    }

    pub fn get_blob_sidecars(
        &self,
        blobs: Vec<Blob>,
        blob_kzg_proofs: Vec<KZGProof>,
    ) -> Vec<BlobSidecar> {
        let signed_block_header = self.compute_signed_block_header();

        let mut blob_sidecars = vec![];

        for (index, blob) in blobs.iter().enumerate() {
            let blob_sidecar = BlobSidecar {
                index: index as u64,
                blob: *blob,
                kzg_commitment: self.message.body.blob_kzg_commitments[index],
                kzg_proof: blob_kzg_proofs[index],
                signed_block_header: signed_block_header.clone(),
                kzg_commitment_inclusion_proof: FixedVector::default(),
            };
            blob_sidecars.push(blob_sidecar);
        }

        blob_sidecars
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlock {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BeaconBlockBody,
}
