use anyhow::anyhow;
use ream_consensus::{
    blob_sidecar::BlobSidecar,
    electra::beacon_block::SignedBeaconBlock,
    execution_engine::rpc_types::get_blobs::{Blob, BlobAndProofV1},
    polynomial_commitments::kzg_proof::KZGProof,
};

use crate::constants::BLOB_SIDECAR_SUBNET_COUNT_ELECTRA;

pub fn get_blob_sidecars(
    signed_block: SignedBeaconBlock,
    blobs: Vec<Blob>,
    blob_kzg_proofs: Vec<KZGProof>,
) -> anyhow::Result<Vec<BlobSidecar>> {
    let mut blob_sidecars = Vec::with_capacity(blobs.len());

    for (index, blob) in blobs.into_iter().enumerate() {
        let blob_and_proof = BlobAndProofV1 {
            blob,
            proof: *blob_kzg_proofs
                .get(index)
                .ok_or_else(|| anyhow!("Kzg_proof not available for blob at index: {index}"))?,
        };

        let blob_sidecar = signed_block.blob_sidecar(blob_and_proof, index as u64)?;

        blob_sidecars.push(blob_sidecar);
    }

    Ok(blob_sidecars)
}

pub fn compute_subnet_for_blob_sidecar(blob_index: u64) -> u64 {
    blob_index % BLOB_SIDECAR_SUBNET_COUNT_ELECTRA
}
