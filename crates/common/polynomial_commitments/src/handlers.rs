use kzg::{DAS, Fr, G1, eip_4844::verify_blob_kzg_proof_batch_raw};
use ream_consensus_beacon::{
    data_column_sidecar::{Cell, MaxBlobCommitmentsPerBlock},
    execution_engine::rpc_types::get_blobs::Blob,
    polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
};
use rust_kzg_blst::{
    eip_7594::BlstBackend,
    types::{fr::FsFr, g1::FsG1, kzg_settings::FsKZGSettings},
};
use ssz_types::VariableList;

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
        .map(|blob| blob.to_fixed_bytes())
        .collect::<Vec<_>>();

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

/// Verify that a set of cells belong to their corresponding commitments.
///
/// Spec: https://ethereum.github.io/consensus-specs/specs/fulu/polynomial-commitments-sampling/#verify_cell_kzg_proof_batch_impl
pub fn verify_cell_kzg_proof_batch(
    commitments_bytes: &VariableList<KZGCommitment, MaxBlobCommitmentsPerBlock>,
    cell_indices: &[u64],
    cells: &VariableList<Cell, MaxBlobCommitmentsPerBlock>,
    proofs_bytes: &VariableList<KZGProof, MaxBlobCommitmentsPerBlock>,
) -> anyhow::Result<bool> {
    // Spec: assert len(commitments_bytes) == len(cells) == len(proofs_bytes) == len(cell_indices)
    if commitments_bytes.len() != cells.len()
        || cells.len() != proofs_bytes.len()
        || proofs_bytes.len() != cell_indices.len()
    {
        anyhow::bail!(
            "Length mismatch: commitments, cells, proofs, and cell_indices must have same length"
        );
    }

    // Spec: for commitment_bytes in commitments_bytes: assert len(commitment_bytes) ==
    // BYTES_PER_COMMITMENT
    for commitment in commitments_bytes.iter() {
        if commitment.0.len() != 48 {
            anyhow::bail!("Invalid commitment length: expected 48 bytes");
        }
    }

    // Spec: for cell_index in cell_indices: assert cell_index < CELLS_PER_EXT_BLOB
    const CELLS_PER_EXT_BLOB: u64 = 128;
    for &cell_index in cell_indices {
        if cell_index >= CELLS_PER_EXT_BLOB {
            anyhow::bail!("Invalid cell index: {cell_index} >= CELLS_PER_EXT_BLOB (128)");
        }
    }

    // Spec: for cell in cells: assert len(cell) == BYTES_PER_CELL
    for cell in cells.iter() {
        if cell.len() != 2048 {
            anyhow::bail!("Invalid cell length: expected 2048 bytes");
        }
    }

    // Spec: for proof_bytes in proofs_bytes: assert len(proof_bytes) == BYTES_PER_PROOF
    for proof in proofs_bytes.iter() {
        if proof.0.len() != 48 {
            anyhow::bail!("Invalid proof length: expected 48 bytes");
        }
    }

    let commitments: Vec<FsG1> = commitments_bytes
        .iter()
        .map(|c| {
            FsG1::from_bytes(&c.0).map_err(|err| anyhow::anyhow!("Invalid commitment bytes: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let cell_indices_usize: Vec<usize> = cell_indices.iter().map(|&i| i as usize).collect();

    // Spec: cosets_evals = [cell_to_coset_evals(cell) for cell in cells]
    let cosets_evals: Vec<FsFr> = cells
        .iter()
        .flat_map(|cell| {
            cell.chunks(32) // BYTES_PER_FIELD_ELEMENT = 32
                .map(|bytes| {
                    FsFr::from_bytes(bytes)
                        .map_err(|err| anyhow::anyhow!("Invalid cell field element: {err}"))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Spec: proofs = [bytes_to_kzg_proof(proof_bytes) for proof_bytes in proofs_bytes]
    let proofs: Vec<FsG1> = proofs_bytes
        .iter()
        .map(|proof| {
            FsG1::from_bytes(&proof.0).map_err(|err| anyhow::anyhow!("Invalid proof bytes: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let settings = trusted_setup::blst_settings();
    <FsKZGSettings as DAS<BlstBackend>>::verify_cell_kzg_proof_batch(
        settings,
        &commitments,
        &cell_indices_usize,
        &cosets_evals,
        &proofs,
    )
    .map_err(|err| anyhow::anyhow!("Cell KZG proof batch verification failed: {err}"))
}
