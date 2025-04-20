use alloy_consensus::Blob;
use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U17};

use crate::{
    beacon_block_header::{SignedBeaconBlockHeader, compute_signed_block_header},
    deneb::beacon_block::SignedBeaconBlock,
    polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
};

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct BlobSidecar {
    pub index: u64,
    pub blob: Blob,
    pub kzg_commitment: KZGCommitment,
    pub kzg_proof: KZGProof,
    pub signed_block_header: SignedBeaconBlockHeader,
    pub kzg_commitment_inclusion_proof: FixedVector<B256, U17>,
}

pub fn get_blob_sidecars(
    signed_block: SignedBeaconBlock,
    blobs: Vec<Blob>,
    blob_kzg_proofs: Vec<KZGProof>,
) -> Vec<BlobSidecar> {
    let signed_block_header = compute_signed_block_header(signed_block.clone());

    let mut blob_sidecars = vec![];

    for (index, blob) in blobs.iter().enumerate() {
        let blob_sidecar = BlobSidecar {
            index: index as u64,
            blob: *blob,
            kzg_commitment: signed_block.message.body.blob_kzg_commitments[index],
            kzg_proof: blob_kzg_proofs[index],
            signed_block_header: signed_block_header.clone(),
            kzg_commitment_inclusion_proof: FixedVector::default(),
        };
        blob_sidecars.push(blob_sidecar);
    }

    blob_sidecars
}

#[derive(Debug, PartialEq, Eq, Hash, Deserialize, Encode, Decode, Ord, PartialOrd)]
pub struct BlobIdentifier {
    pub block_root: B256,
    pub index: u64,
}

impl BlobIdentifier {
    pub fn new(block_root: B256, index: u64) -> Self {
        Self { block_root, index }
    }
}
