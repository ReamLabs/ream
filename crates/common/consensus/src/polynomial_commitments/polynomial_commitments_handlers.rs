use kzg::{
    eip_4844::verify_blob_kzg_proof_batch_raw,
    eth::c_bindings::{Bytes48, KZGProof},
};
use ream_bls::{PubKey, traits::Validate};

use super::{
    error::KzgError,
    kzg_proof::{Bytes48 as OtherBytes48, KZGProof as OtherKZGProof},
    trusted_setup,
};
use crate::{execution_engine::rpc_types::get_blobs::Blob, kzg_commitment::KZGCommitment};

/// Perform BLS validation required by the types `KZGProof` and `KZGCommitment`.
pub fn validate_kzg_g1(pubkey: &PubKey) -> anyhow::Result<()> {
    if *pubkey == PubKey::infinity() {
        return Ok(());
    }

    Ok(pubkey.validate()?)
}

/// Convert untrusted bytes into a trusted and validated KZGCommitment.
pub fn bytes_to_kzg_commitment(pubkey: PubKey) -> anyhow::Result<KZGCommitment> {
    validate_kzg_g1(&pubkey)?;

    let mut fixed_array = [0u8; 48];
    fixed_array.copy_from_slice(&pubkey.inner);
    Ok(KZGCommitment(fixed_array))
}

/// Convert untrusted bytes into a trusted and validated KZGProof.
pub fn bytes_to_kzg_proof(pubkey: PubKey) -> anyhow::Result<KZGProof> {
    validate_kzg_g1(&pubkey)?;

    let mut fixed_array = [0u8; 48];
    fixed_array.copy_from_slice(&pubkey.inner);
    Ok(KZGProof {
        bytes: (fixed_array),
    })
}

/// Given a list of blobs and blob KZG proofs, verify that they correspond to the provided
/// commitments. Will return True if there are zero blobs/commitments/proofs.
/// Public method.
pub fn verify_blob_kzg_proof_batch(
    blobs: &[Blob],
    commitments_bytes: Vec<KZGCommitment>,
    proofs_bytes: &[OtherKZGProof],
) -> anyhow::Result<bool> {
    let raw_blobs = blobs
        .iter()
        .map(|blob| {
            let blob: [u8; 131072] = (*blob).into();
            Ok(blob)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let raw_commitments = commitments_bytes
        .into_iter()
        .map(Bytes48::from)
        .collect::<Vec<_>>();

    let raw_proofs = proofs_bytes
        .iter()
        .map(OtherBytes48::from)
        .collect::<Vec<_>>();

    let result = verify_blob_kzg_proof_batch_raw(
        &raw_blobs,
        &raw_commitments
            .iter()
            .map(|commitments| commitments.bytes)
            .collect::<Vec<_>>(),
        &raw_proofs
            .iter()
            .map(|proofs| proofs.bytes)
            .collect::<Vec<_>>(),
        trusted_setup::blst_settings(),
    );

    result.map_err(KzgError::KzgError).map_err(Into::into)
}
