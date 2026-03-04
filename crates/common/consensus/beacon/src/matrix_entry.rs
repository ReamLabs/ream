use alloy_primitives::B256;
use anyhow::{Ok, Result, anyhow, ensure};
use discv5::enr::k256::{
    Scalar,
    elliptic_curve::{Field, PrimeField},
};
use ream_consensus_misc::{
    constants::beacon::{
        BYTES_PER_BLOB, BYTES_PER_FIELD_ELEMENT, CELLS_PER_EXT_BLOB, FIELD_ELEMENTS_PER_BLOB,
        FIELD_ELEMENTS_PER_CELL, FIELD_ELEMENTS_PER_EXT_BLOB,
    },
    polynomial_commitments::kzg_proof::KZGProof,
};
use ream_execution_rpc_types::get_blobs::Blob;
use rust_eth_kzg::{Cell as KZGCell, CellIndex, DASContext, KZGProof as Proof};
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

/// Convert untrusted bytes to a trusted and validated BLS scalar field element.
/// This function does not accept inputs greater than the BLS modulus.
pub fn bytes_to_bls_field(b: B256) -> anyhow::Result<Scalar> {
    ensure!(b.len() == 32, "Invalid input length for field element");

    let field_element = Scalar::from_repr((*b).into())
        .into_option()
        .ok_or_else(|| anyhow!("Bytes exceed the field modulus"))?;
    Ok(field_element)
}

/// Convert a blob to list of BLS field scalars.
pub fn blob_to_polynomial(blob: Blob) -> anyhow::Result<[Scalar; FIELD_ELEMENTS_PER_BLOB]> {
    let mut polynomial = [Scalar::default(); FIELD_ELEMENTS_PER_BLOB];

    for (i, polynomial_element) in polynomial.iter_mut().enumerate() {
        let start = i * BYTES_PER_FIELD_ELEMENT;
        let end = (i + 1) * BYTES_PER_FIELD_ELEMENT;

        let chunk: B256 = blob.inner[start..end]
            .try_into()
            .map_err(|err| anyhow!("Invalid chunk size at index {err:?}"))?;

        *polynomial_element = bytes_to_bls_field(chunk)?;
    }

    Ok(polynomial)
}

/// Return ``x`` to power of [0, n-1], if n > 0. When n==0, an empty array is returned.
pub fn compute_powers(x: Scalar, n: u64) -> anyhow::Result<Vec<Scalar>> {
    let mut powers = Vec::with_capacity(n as usize);
    let mut current_power = Scalar::ONE;

    for _ in 0..n {
        powers.push(current_power);
        current_power *= x;
    }

    Ok(powers)
}

/// Return roots of unity of ``order``.
pub fn compute_roots_of_unity(order: u64) -> anyhow::Result<Vec<Scalar>> {
    ensure!(
        order <= (1 << Scalar::S),
        "Order exceeds maximum supported by field"
    );
    let ratio = (1 << Scalar::S) / order;
    let exponent: [u64; 4] = [ratio, 0, 0, 0];

    let root = Scalar::ROOT_OF_UNITY.pow_vartime(exponent);

    Ok(compute_powers(root, order)?)
}

fn _fft_field(vals: Vec<Scalar>, roots_of_unity: Vec<Scalar>) -> anyhow::Result<Vec<Scalar>> {
    let n = vals.len();
    if n <= 1 {
        return Ok(vals);
    }
    let mut evens = Vec::with_capacity(n / 2);
    let mut odds = Vec::with_capacity(n / 2);
    for (i, val) in vals.into_iter().enumerate() {
        if i % 2 == 0 {
            evens.push(val);
        } else {
            odds.push(val);
        }
    }
    let next_roots: Vec<Scalar> = roots_of_unity.iter().step_by(2).cloned().collect();
    let left = _fft_field(evens, next_roots.clone())?;
    let right = _fft_field(odds, next_roots)?;

    let mut result = vec![Scalar::ZERO; n];
    for i in 0..(n / 2) {
        let root = roots_of_unity[i];
        let y_times_root = right[i] * root;
        result[i] = left[i] + y_times_root;
        result[i + (n / 2)] = left[i] - y_times_root;
    }

    Ok(result)
}

pub fn fft_field(
    vals: Vec<Scalar>,
    roots_of_unity: Vec<Scalar>,
    inv: bool,
) -> anyhow::Result<Vec<Scalar>> {
    if inv {
        let n = vals.len() as u64;
        let invlen = Scalar::from(n).invert().unwrap();

        let mut reversed_roots = Vec::with_capacity(roots_of_unity.len());
        if !roots_of_unity.is_empty() {
            reversed_roots.push(roots_of_unity[0]);
            reversed_roots.extend(roots_of_unity.iter().skip(1).rev());
        }

        let fft_result = _fft_field(vals, reversed_roots)?;
        let result = fft_result.into_iter().map(|x| x * invlen).collect();

        Ok(result)
    } else {
        Ok(_fft_field(vals, roots_of_unity)?)
    }
}

/// Reverse the bit order of an integer ``n``.
pub fn reverse_bits(n: usize, order: usize) -> anyhow::Result<usize> {
    ensure!(order.is_power_of_two(), "Order must be a power of two");

    let width = order.trailing_zeros();
    let mut result = 0;
    let mut temp_n = n;

    for _ in 0..width {
        result = (result << 1) | (temp_n & 1);
        temp_n >>= 1;
    }

    Ok(result)
}

/// Return a copy with bit-reversed permutation. The permutation is an involution (inverts itself).
///
/// The input and output are a sequence of generic type ``T`` objects.
pub fn bit_reversal_permutation<T: Clone>(sequence: Vec<T>) -> anyhow::Result<Vec<T>> {
    let n = sequence.len();
    let mut result = Vec::with_capacity(n);

    for i in 0..n {
        let rev_idx = reverse_bits(i, n)?;
        result.push(sequence[rev_idx].clone());
    }

    Ok(result)
}

/// Interpolates a polynomial (given in evaluation form) to a polynomial in coefficient form.
pub fn polynomial_eval_to_coeff(
    polynomial: [Scalar; FIELD_ELEMENTS_PER_BLOB],
) -> anyhow::Result<Vec<Scalar>> {
    let roots_of_unity = compute_roots_of_unity(FIELD_ELEMENTS_PER_BLOB as u64)?;
    let rearranged = bit_reversal_permutation(polynomial.to_vec())?;

    fft_field(rearranged, roots_of_unity, true)
}

/// Get the coset for a given ``cell_index``.
/// Precisely, consider the group of roots of unity of order FIELD_ELEMENTS_PER_CELL *
/// CELLS_PER_EXT_BLOB. Let G = {1, g, g^2, ...} denote its subgroup of order
/// FIELD_ELEMENTS_PER_CELL. Then, the coset is defined as h * G = {h, hg, hg^2, ...}.
/// This function, returns the coset.
pub fn coset_for_cell(cell_index: CellIndex) -> anyhow::Result<[Scalar; FIELD_ELEMENTS_PER_CELL]> {
    ensure!(
        cell_index < CELLS_PER_EXT_BLOB,
        "Cell index great then CELLS_PER_EXT_BLOB"
    );
    let roots = compute_roots_of_unity(FIELD_ELEMENTS_PER_EXT_BLOB as u64)?;
    let roots_of_unity_brp = bit_reversal_permutation(roots)?;

    let start = (cell_index as usize) * FIELD_ELEMENTS_PER_CELL;
    let end = start + FIELD_ELEMENTS_PER_CELL;

    let coset_slice = &roots_of_unity_brp[start..end];

    let coset: [Scalar; FIELD_ELEMENTS_PER_CELL] = coset_slice
        .try_into()
        .map_err(|err| anyhow!("Slice length mismatch for coset conversion {err:?}"))?;

    Ok(coset)
}

/// Evaluate a coefficient form polynomial at ``z`` using Horner's schema.
pub fn evaluate_polynomialcoeff(polynomial_coeff: &[Scalar], z: Scalar) -> anyhow::Result<Scalar> {
    let mut y = Scalar::ZERO;
    for coef in polynomial_coeff.iter().rev() {
        y = (y * z) + *coef;
    }

    Ok(y)
}

pub fn bls_field_to_bytes(x: Scalar) -> B256 {
    let bytes: [u8; 32] = x.to_bytes().into();
    B256::from(bytes)
}

/// Convert a trusted ``CosetEval`` into an untrusted ``Cell``.
pub fn coset_evals_to_cell(coset_evals: [Scalar; FIELD_ELEMENTS_PER_CELL]) -> anyhow::Result<Cell> {
    let mut cell_bytes = Vec::with_capacity(FIELD_ELEMENTS_PER_CELL * 32);
    for eval in coset_evals {
        let bytes: B256 = bls_field_to_bytes(eval);
        cell_bytes.extend_from_slice(bytes.as_slice());
    }

    let cell = cell_bytes
        .try_into()
        .map_err(|err| anyhow!("Failed to convert bytes to Cell FixedVector {err:?}"))?;
    Ok(cell)
}

pub fn compute_cells(blob: Blob) -> anyhow::Result<[Cell; CELLS_PER_EXT_BLOB as usize]> {
    ensure!(blob.inner.len() == BYTES_PER_BLOB, "Invalid blob length");
    let polynomial = blob_to_polynomial(blob)?;
    let polynomial_coeff = polynomial_eval_to_coeff(polynomial)?;
    let mut cells = Vec::with_capacity(CELLS_PER_EXT_BLOB as usize);

    for i in 0..CELLS_PER_EXT_BLOB {
        let coset = coset_for_cell(i as CellIndex)?;

        let mut ys = [Scalar::ZERO; FIELD_ELEMENTS_PER_CELL];
        for (j, &z) in coset.iter().enumerate() {
            ys[j] = evaluate_polynomialcoeff(&polynomial_coeff, z)?;
        }

        let cell = coset_evals_to_cell(ys)?;
        cells.push(cell);
    }

    let final_cells: [Cell; CELLS_PER_EXT_BLOB as usize] = cells
        .try_into()
        .map_err(|err| anyhow!("Failed to convert cells to fixed array {err:?}"))?;

    Ok(final_cells)
}
