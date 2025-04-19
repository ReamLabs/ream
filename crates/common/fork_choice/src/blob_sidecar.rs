use alloy_consensus::Blob;
use alloy_primitives::B256;
use ream_consensus::{
    beacon_block_header::SignedBeaconBlockHeader,
    polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
};
use serde::Deserialize;
use ssz_types::{FixedVector, typenum::U17};

type KzgCommitmentInclusionProofDepth = U17;

#[derive(Debug, PartialEq, Deserialize)]
pub struct BlobSidecar {
    pub index: u64,
    pub blob: Blob,
    pub kzg_commitment: KZGCommitment,
    pub kzg_proof: KZGProof,
    pub signed_block_header: SignedBeaconBlockHeader,
    pub kzg_commitment_inclusion_proof: FixedVector<B256, KzgCommitmentInclusionProofDepth>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct BlobIdentifier {
    pub block_root: B256,
    pub index: u64,
}
