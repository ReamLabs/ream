use alloy_primitives::B256;
use ream_consensus_misc::{
    beacon_block_header::SignedBeaconBlockHeader,
    constants::beacon::{BLOB_KZG_COMMITMENTS_INDEX, DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH},
    polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
};
use ream_merkle::is_valid_merkle_branch;
use ream_network_spec::networks::beacon_network_spec;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, VariableList, typenum};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::error::DataColumnSidecarError;

pub type Cell = FixedVector<u8, typenum::U2048>;

pub const NUMBER_OF_COLUMNS: u64 = 128;
pub const DATA_COLUMN_SIDECAR_SUBNET_COUNT: u64 = 128;

pub type MaxBlobCommitmentsPerBlock = typenum::U4096;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Encode, Decode, TreeHash)]
pub struct DataColumnSidecar {
    pub index: u64,
    pub column: VariableList<Cell, MaxBlobCommitmentsPerBlock>,
    pub kzg_commitments: VariableList<KZGCommitment, MaxBlobCommitmentsPerBlock>,
    pub kzg_proofs: VariableList<KZGProof, MaxBlobCommitmentsPerBlock>,
    pub signed_block_header: SignedBeaconBlockHeader,
    pub kzg_commitments_inclusion_proof: FixedVector<B256, typenum::U4>,
}

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Deserialize,
    Serialize,
    Encode,
    Decode,
    Default,
    Ord,
    PartialOrd,
)]
pub struct ColumnIdentifier {
    pub block_root: B256,
    pub index: u64,
}

impl ColumnIdentifier {
    pub fn new(block_root: B256, index: u64) -> Self {
        Self { block_root, index }
    }
}

impl DataColumnSidecar {
    pub fn compute_subnet(&self) -> u64 {
        self.index % DATA_COLUMN_SIDECAR_SUBNET_COUNT
    }

    /// Verifies that the kzg_commitments list is included in the block body
    pub fn verify_inclusion_proof(&self) -> bool {
        is_valid_merkle_branch(
            self.kzg_commitments.tree_hash_root(),
            &self.kzg_commitments_inclusion_proof,
            DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH,
            BLOB_KZG_COMMITMENTS_INDEX,
            self.signed_block_header.message.body_root,
        )
    }

    /// Verify if the data column sidecar is valid.
    ///
    /// Spec: https://ethereum.github.io/consensus-specs/specs/fulu/p2p-interface/#verify_data_column_sidecar
    pub fn verify(&self) -> bool {
        // The sidecar index must be within the valid range
        if self.index >= NUMBER_OF_COLUMNS {
            return false;
        }

        // A sidecar for zero blobs is invalid
        if self.kzg_commitments.is_empty() {
            return false;
        }

        // Check that the sidecar respects the blob limit
        let max_blobs_per_block = beacon_network_spec().max_blobs_per_block_electra as usize;
        if self.kzg_commitments.len() > max_blobs_per_block {
            return false;
        }

        // The column length must be equal to the number of commitments/proofs
        if self.column.len() != self.kzg_commitments.len()
            || self.column.len() != self.kzg_proofs.len()
        {
            return false;
        }

        true
    }
}

/// https://github.com/ethereum/consensus-specs/blob/master/specs/fulu/validator.md#get_data_column_sidecars
pub fn get_data_column_sidecars(
    signed_block_header: SignedBeaconBlockHeader,
    kzg_commitments: VariableList<KZGCommitment, MaxBlobCommitmentsPerBlock>,
    kzg_commitments_inclusion_proof: FixedVector<B256, typenum::U4>,
    cells_and_kzg_proofs: Vec<(Vec<Cell>, Vec<KZGProof>)>,
) -> Result<Vec<DataColumnSidecar>, DataColumnSidecarError> {
    if cells_and_kzg_proofs.len() != kzg_commitments.len() {
        return Err(DataColumnSidecarError::CommitmentCountMismatch {
            actual: cells_and_kzg_proofs.len(),
            expected: kzg_commitments.len(),
        });
    }

    let mut sidecars = Vec::new();
    for column_index in 0..NUMBER_OF_COLUMNS {
        let mut column_cells = Vec::new();
        let mut column_proofs = Vec::new();
        for (cells, proofs) in &cells_and_kzg_proofs {
            if column_index as usize >= cells.len() || column_index as usize >= proofs.len() {
                return Err(DataColumnSidecarError::ColumnIndexOutOfBounds(
                    column_index as usize,
                    cells.len().max(proofs.len()),
                ));
            }
            column_cells.push(cells[column_index as usize].clone());
            column_proofs.push(proofs[column_index as usize]);
        }

        sidecars.push(DataColumnSidecar {
            index: column_index,
            column: VariableList::try_from(column_cells).map_err(|err| {
                DataColumnSidecarError::DecodingError {
                    column_index,
                    err: err.to_string(),
                }
            })?,
            kzg_commitments: kzg_commitments.clone(),
            kzg_proofs: VariableList::try_from(column_proofs).map_err(|err| {
                DataColumnSidecarError::DecodingError {
                    column_index,
                    err: err.to_string(),
                }
            })?,
            signed_block_header: signed_block_header.clone(),
            kzg_commitments_inclusion_proof: kzg_commitments_inclusion_proof.clone(),
        });
    }
    Ok(sidecars)
}

/// Reconstructs the data column sidecars from any received column sidecar and the corresponding
/// cells and KZG proofs for each commitment.
///
/// Spec: https://github.com/ethereum/consensus-specs/blob/master/specs/fulu/validator.md#get_data_column_sidecars_from_column_sidecar
pub fn get_data_column_sidecars_from_column_sidecar(
    sidecar: DataColumnSidecar,
    cells_and_kzg_proofs: Vec<(Vec<Cell>, Vec<KZGProof>)>,
) -> Result<Vec<DataColumnSidecar>, DataColumnSidecarError> {
    if cells_and_kzg_proofs.len() != sidecar.kzg_commitments.len() {
        return Err(DataColumnSidecarError::CommitmentCountMismatch {
            actual: sidecar.kzg_commitments.len(),
            expected: cells_and_kzg_proofs.len(),
        });
    }

    get_data_column_sidecars(
        sidecar.signed_block_header,
        sidecar.kzg_commitments,
        sidecar.kzg_commitments_inclusion_proof,
        cells_and_kzg_proofs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_subnet() {
        let mut sidecar = DataColumnSidecar {
            index: 0,
            column: VariableList::empty(),
            kzg_commitments: VariableList::empty(),
            kzg_proofs: VariableList::empty(),
            signed_block_header: SignedBeaconBlockHeader::default(),
            kzg_commitments_inclusion_proof: FixedVector::default(),
        };

        assert_eq!(sidecar.compute_subnet(), 0);

        sidecar.index = 127;
        assert_eq!(sidecar.compute_subnet(), 127);

        sidecar.index = 128;
        assert_eq!(sidecar.compute_subnet(), 0);

        sidecar.index = 255;
        assert_eq!(sidecar.compute_subnet(), 127);
    }
}
