use kzg::eip_4844::verify_blob_kzg_proof_batch_raw;
use ream_consensus::{
    execution_engine::rpc_types::get_blobs::Blob, kzg_commitment::KZGCommitment,
    polynomial_commitments::kzg_proof::KZGProof,
};

use super::{error::KzgError, trusted_setup};

/// Given a list of blobs and blob KZG proofs, verify that they correspond to the provided
/// commitments. Will return True if there are zero blobs/commitments/proofs.
/// Public method.
pub fn verify_blob_kzg_proof_batch(
    blobs: &[Blob],
    commitments_bytes: &[KZGCommitment],
    proofs_bytes: &[KZGProof],
) -> anyhow::Result<bool> {
    let raw_blobs = blobs
        .iter()
        .map(|blob| {
            let blob: [u8; 131072] = (*blob).into();
            Ok(blob)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let raw_commitments = commitments_bytes
        .iter()
        .map(KZGCommitment::to_fixed_bytes)
        .collect::<Vec<_>>();

    let raw_proofs = proofs_bytes
        .iter()
        .map(KZGProof::to_fixed_bytes)
        .collect::<Vec<_>>();

    let result = verify_blob_kzg_proof_batch_raw(
        &raw_blobs,
        &raw_commitments,
        &raw_proofs,
        trusted_setup::blst_settings(),
    );

    result.map_err(KzgError::KzgError).map_err(Into::into)
}
