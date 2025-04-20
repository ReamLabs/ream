use alloy_consensus::Blob;
use kzg::eip_4844::verify_blob_kzg_proof_batch_raw;
use ream_consensus::polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof};

use super::{error::KzgError, trusted_setup};

/// Given a list of blobs and blob KZG proofs, verify that they correspond to the provided
/// commitments. Will return True if there are zero blobs/commitments/proofs.
/// Public method.
pub fn verify_blob_kzg_proof_batch(
    blobs: &[Blob],
    commitments_bytes: &[KZGCommitment],
    proofs_bytes: &[KZGProof],
) -> anyhow::Result<bool> {
    let raw_blobs = blobs.iter().map(|blob| blob.0).collect::<Vec<_>>();

    let raw_commitments = commitments_bytes
        .iter()
        .map(|commitment| commitment.0)
        .collect::<Vec<_>>();

    let raw_proofs = proofs_bytes.iter().map(|proof| proof.0).collect::<Vec<_>>();

    let result = verify_blob_kzg_proof_batch_raw(
        &raw_blobs,
        &raw_commitments,
        &raw_proofs,
        trusted_setup::blst_settings(),
    );

    result.map_err(KzgError::KzgError).map_err(Into::into)
}
