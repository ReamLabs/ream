use alloy_consensus::Blob;
use alloy_primitives::B256;
use serde::Deserialize;
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U17};

use crate::{
    beacon_block_header::SignedBeaconBlockHeader,
    polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
};

#[derive(Debug, PartialEq, Deserialize)]
pub struct BlobSidecar {
    pub index: u64,
    pub blob: Blob,
    pub kzg_commitment: KZGCommitment,
    pub kzg_proof: KZGProof,
    pub signed_block_header: SignedBeaconBlockHeader,
    pub kzg_commitment_inclusion_proof: FixedVector<B256, U17>,
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
