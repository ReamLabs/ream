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
    let mut matrix = Vec::new();

    for (blob_index, blob) in blobs.iter().enumerate() {
        let (cells, proofs) = compute_cells_and_kzg_proofs(blob, das_context)?;
        for (cell_index, (cell, kzg_proof)) in cells.into_iter().zip(proofs.into_iter()).enumerate()
        {
            matrix.push(MatrixEntry {
                cell,
                kzg_proof,
                column_index: cell_index as u64,
                row_index: blob_index as u64,
            });
        }
    }

    Ok(matrix)
}

pub fn recover_matrix(
    partial_matrix: Vec<MatrixEntry>,
    blob_count: u64,
    das_context: &DASContext,
) -> Result<Vec<MatrixEntry>> {
    let mut matrix = Vec::new();

    for blob_index in 0..blob_count {
        let (cell_indices, cells): (Vec<u64>, Vec<Cell>) = partial_matrix
            .iter()
            .filter(|entry| entry.row_index == blob_index)
            .map(|entry| (entry.column_index, entry.cell.clone()))
            .unzip();

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
    let blob_data: Vec<u8> = blob.inner.clone().into();
    if blob_data.len() != 131072 {
        return Err(anyhow!(
            "Blob inner length {}, expected 131072",
            blob_data.len()
        ));
    }
    let blob_bytes: &[u8; 131072] = blob_data
        .as_slice()
        .try_into()
        .map_err(|err| anyhow!("Failed to convert blob inner to &[u8; 131072]: {err}"))?;
    let (kzg_cells, kzg_proofs) = das_context
        .compute_cells_and_kzg_proofs(blob_bytes)
        .map_err(|err| anyhow!("KZG error: {err:?}"))?;

    let cells = kzg_cells.into_iter().map(convert_cell).collect();
    let proofs = kzg_proofs.into_iter().map(convert_kzg_proof).collect();

    Ok((cells, proofs))
}

fn recover_cells_and_kzg_proofs(
    cell_indices: Vec<u64>,
    cells: Vec<Cell>,
    das_context: &DASContext,
) -> Result<(Vec<Cell>, Vec<KZGProof>)> {
    let kzg_cells_result: Result<Vec<[u8; 2048]>> = cells
        .into_iter()
        .map(|cell_fixed_vector| {
            let cell_vec: Vec<u8> = cell_fixed_vector.into();
            if cell_vec.len() != 2048 {
                return Err(anyhow!("Cell length {}, expected 2048", cell_vec.len()));
            }
            let cell_array: [u8; 2048] = cell_vec
                .try_into()
                .map_err(|err| anyhow!("Failed to convert Cell to [u8; 2048]: {err:?}"))?;
            Ok(cell_array)
        })
        .collect();

    let kzg_cells = kzg_cells_result?;
    let kzg_cells_refs = kzg_cells.iter().collect();
    let (new_kzg_cells, new_kzg_proofs) = das_context
        .recover_cells_and_kzg_proofs(cell_indices, kzg_cells_refs)
        .map_err(|err| anyhow!("KZG recovery error: {err:?}"))?;

    let cells = new_kzg_cells.into_iter().map(convert_cell).collect();
    let proofs = new_kzg_proofs.into_iter().map(convert_kzg_proof).collect();

    Ok((cells, proofs))
}

fn convert_cell(kzg_cell: KZGCell) -> Cell {
    FixedVector::try_from(kzg_cell.to_vec()).expect("Cell conversion failed")
}

fn convert_kzg_proof(kzg_proof: Proof) -> KZGProof {
    KZGProof::from(kzg_proof)
}
