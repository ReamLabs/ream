use anyhow::{Ok, Result, anyhow, ensure};
use ream_consensus_misc::{
    constants::beacon::CELLS_PER_EXT_BLOB, polynomial_commitments::kzg_proof::KZGProof,
};
use ream_execution_rpc_types::get_blobs::Blob;
use rust_eth_kzg::{Cell as KZGCell, DASContext, KZGProof as Proof};
use ssz_types::FixedVector;

use crate::data_column_sidecar::Cell;

#[derive(Debug, Clone, PartialEq)]
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
        for (cell_index, (cell, kzg_proof)) in cells.into_iter().zip(proofs).enumerate() {
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
            .zip(recovered_proofs)
            .enumerate()
        {
            matrix.push(MatrixEntry {
                cell,
                kzg_proof,
                column_index: cell_index as u64,
                row_index: blob_index,
            });
        }
    }

    Ok(matrix)
}

pub fn compute_cells_and_kzg_proofs(
    blob: &Blob,
    das_context: &DASContext,
) -> Result<(Vec<Cell>, Vec<KZGProof>)> {
    let blob_data: Vec<u8> = blob.inner.clone().into();
    ensure!(
        blob_data.len() == 131072,
        "Blob inner length {}, expected 131072",
        blob_data.len()
    );
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

pub fn recover_cells_and_kzg_proofs(
    cell_indices: Vec<u64>,
    cells: Vec<Cell>,
    das_context: &DASContext,
) -> Result<(Vec<Cell>, Vec<KZGProof>)> {
    let kzg_cells_result: Result<Vec<[u8; 2048]>> = cells
        .into_iter()
        .map(|cell_fixed_vector| {
            let cell_vec: Vec<u8> = cell_fixed_vector.into();
            ensure!(
                cell_vec.len() == 2048,
                "Cell length {}, expected 2048",
                cell_vec.len()
            );
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

pub fn compute_cells(
    blob: Blob,
    das_context: &DASContext,
) -> anyhow::Result<[Cell; CELLS_PER_EXT_BLOB as usize]> {
    let blob_data: Vec<u8> = blob.inner.into();
    let blob_bytes: &[u8; 131072] = blob_data
        .as_slice()
        .try_into()
        .map_err(|err| anyhow!("Invalid blob size {err:?}"))?;

    let kzg_cells = das_context
        .compute_cells(blob_bytes)
        .map_err(|err| anyhow!("KZG error: {err:?}"))?;

    let cells: Vec<Cell> = kzg_cells.into_iter().map(convert_cell).collect();

    let final_cells: [Cell; CELLS_PER_EXT_BLOB as usize] = cells
        .try_into()
        .map_err(|err| anyhow!("Failed to convert to fixed array {err:?}"))?;

    Ok(final_cells)
}

#[cfg(test)]
mod tests {
    use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};
    use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
    use ssz::Decode;

    use super::*;

    const BYTES_PER_BLOB: usize = 131072;

    fn get_sample_blob(rng: &mut StdRng) -> Blob {
        let mut bytes = vec![0u8; BYTES_PER_BLOB];
        for chunk in bytes.chunks_mut(32) {
            rng.fill(&mut chunk[1..]);
        }
        Blob::from_ssz_bytes(&bytes).expect("constructed blob bytes should decode")
    }

    fn chunks<T: Clone>(items: &[T], size: usize) -> Vec<Vec<T>> {
        items.chunks(size).map(|chunk| chunk.to_vec()).collect()
    }

    #[test]
    fn test_compute_matrix() -> Result<()> {
        let mut rng = StdRng::seed_from_u64(5566);
        let context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);

        let blob_count = 2;
        let input_blobs: Vec<Blob> = (0..blob_count).map(|_| get_sample_blob(&mut rng)).collect();

        let matrix = compute_matrix(input_blobs.clone(), &context)?;
        assert_eq!(matrix.len(), CELLS_PER_EXT_BLOB as usize * blob_count);

        let rows = chunks(&matrix, CELLS_PER_EXT_BLOB as usize);
        assert_eq!(rows.len(), blob_count);
        for row in &rows {
            assert_eq!(row.len(), CELLS_PER_EXT_BLOB as usize);
        }

        for (blob_index, row) in rows.iter().enumerate() {
            let mut column_indices: Vec<u64> = row
                .iter()
                .map(|entry| {
                    assert_eq!(entry.row_index, blob_index as u64);
                    entry.column_index
                })
                .collect();
            column_indices.sort();
            assert_eq!(column_indices, (0..CELLS_PER_EXT_BLOB).collect::<Vec<_>>());

            let mut extended_blob = Vec::<u8>::new();
            for entry in row {
                extended_blob.extend::<Vec<u8>>(entry.cell.clone().into());
            }
            let blob_part = &extended_blob[..extended_blob.len() / 2];
            let original: Vec<u8> = input_blobs[blob_index].inner.clone().into();
            assert_eq!(blob_part, original.as_slice());
        }

        Ok(())
    }

    #[test]
    fn test_recover_matrix() -> Result<()> {
        let mut rng = StdRng::seed_from_u64(5566);
        let context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);

        let n_samples = (CELLS_PER_EXT_BLOB / 2) as usize;
        let blob_count = 2;

        let blobs: Vec<Blob> = (0..blob_count).map(|_| get_sample_blob(&mut rng)).collect();
        let matrix = compute_matrix(blobs, &context)?;

        let mut partial_matrix = Vec::new();
        for blob_entries in chunks(&matrix, CELLS_PER_EXT_BLOB as usize) {
            let mut indices: Vec<usize> = (0..blob_entries.len()).collect();

            indices.shuffle(&mut rng);
            let mut sampled = indices[..n_samples].to_vec();
            sampled.sort();
            partial_matrix.extend(sampled.into_iter().map(|i| blob_entries[i].clone()));
        }

        let recovered_matrix = recover_matrix(partial_matrix, blob_count as u64, &context)?;

        assert_eq!(recovered_matrix, matrix);

        Ok(())
    }
}
