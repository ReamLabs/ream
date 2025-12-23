use anyhow::{Ok, Result, anyhow};
use ream_consensus_misc::polynomial_commitments::kzg_proof::KZGProof;
use ream_execution_rpc_types::get_blobs::Blob;
use rust_eth_kzg::{Cell as KZGCell, DASContext, KZGProof as Proof};
use ssz_types::FixedVector;

use crate::data_column_sidecar::Cell;

#[derive(Debug, Clone)]
pub struct MatrixEntry {
    cell: Cell,
    #[allow(dead_code)]
    kzg_proof: KZGProof,
    column_index: u64,
    row_index: u64,
}

pub fn compute_matrix(blobs: Vec<Blob>, das_context: &DASContext) -> Result<Vec<MatrixEntry>> {
    let mut matrix: Vec<MatrixEntry> = Vec::new();

    for (blob_index, blob) in blobs.iter().enumerate() {
        let (cells, proofs) = compute_cells_and_kzg_proofs(blob, das_context)?;
        for (cell_index, (cell, kzg_proof)) in cells.into_iter().zip(proofs.into_iter()).enumerate()
        {
            matrix.push(MatrixEntry {
                cell,
                kzg_proof,
                column_index: blob_index as u64,
                row_index: cell_index as u64,
            });
        }
    }

    Ok(matrix)
}

pub fn remove_matrix(
    partial_matrix: Vec<MatrixEntry>,
    blob_count: u64,
    das_context: &DASContext,
) -> Result<Vec<MatrixEntry>> {
    let mut matrix: Vec<MatrixEntry> = Vec::new();

    for blob_index in 0..blob_count {
        let cell_indices: Vec<u64> = partial_matrix
            .iter()
            .filter(|e| e.row_index == blob_index)
            .map(|e| e.column_index)
            .collect();
        let cells: Vec<Cell> = partial_matrix
            .iter()
            .filter(|e| e.row_index == blob_index)
            .map(|e| e.cell.clone())
            .collect();

        let (recovered_cells, recovered_proofs) =
            recover_cells_and_kzg_proofs(cell_indices, cells, das_context)?;

        for (cell_index, (cell, kzg_proof)) in recovered_cells
            .into_iter()
            .zip(recovered_proofs.into_iter())
            .enumerate()
        {
            matrix.push(MatrixEntry {
                cell,
                kzg_proof,
                column_index: blob_index,
                row_index: cell_index as u64,
            });
        }
    }

    Ok(matrix)
}

fn compute_cells_and_kzg_proofs(
    blob: &Blob,
    das_context: &DASContext,
) -> Result<(Vec<Cell>, Vec<KZGProof>)> {
    let bytes: Vec<u8> = blob.inner.clone().into();
    if bytes.len() != 131072 {
        return Err(anyhow!(
            "Blob inner length {}, expected 131072",
            bytes.len()
        ));
    }
    let arr: &[u8; 131072] = bytes
        .as_slice()
        .try_into()
        .map_err(|err| anyhow!("Failed to convert blob inner to &[u8; 131072]: {err}"))?;
    let (kzg_cells, kzg_proofs) = das_context
        .compute_cells_and_kzg_proofs(arr)
        .map_err(|err| anyhow!("KZG error: {err:?}"))?;

    let cells: Vec<Cell> = kzg_cells.into_iter().map(convert_cell).collect();
    let proofs: Vec<KZGProof> = kzg_proofs.into_iter().map(convert_kzg_proof).collect();

    Ok((cells, proofs))
}

fn recover_cells_and_kzg_proofs(
    cell_indices: Vec<u64>,
    cells: Vec<Cell>,
    das_context: &DASContext,
) -> Result<(Vec<Cell>, Vec<KZGProof>)> {
    let kzg_cells: Result<Vec<[u8; 2048]>> = cells
        .into_iter()
        .map(|fv| {
            let vec: Vec<u8> = fv.into();
            if vec.len() != 2048 {
                return Err(anyhow!("Cell length {}, expected 2048", vec.len()));
            }
            let arr: [u8; 2048] = vec
                .try_into()
                .map_err(|err| anyhow!("Failed to convert Cell to [u8; 2048]: {err:?}"))?;
            Ok(arr)
        })
        .collect();

    let kzg_cells = kzg_cells?;
    let kzg_cells_refs: Vec<&[u8; 2048]> = kzg_cells.iter().collect();
    let (new_kzg_cells, new_kzg_proofs) = das_context
        .recover_cells_and_kzg_proofs(cell_indices, kzg_cells_refs)
        .map_err(|err| anyhow!("KZG recovery error: {err:?}"))?;

    let cells: Vec<Cell> = new_kzg_cells.into_iter().map(convert_cell).collect();
    let proofs: Vec<KZGProof> = new_kzg_proofs.into_iter().map(convert_kzg_proof).collect();

    Ok((cells, proofs))
}

fn convert_cell(kzg_cell: KZGCell) -> Cell {
    let array_ref: &[u8; 2048] = &kzg_cell;
    FixedVector::try_from(array_ref.to_vec()).expect("Cell conversion failed")
}

fn convert_kzg_proof(kzg_proof: Proof) -> KZGProof {
    KZGProof::from(kzg_proof)
}
