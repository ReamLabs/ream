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

use crate::{electra::beacon_block::SignedBeaconBlock, error::DataColumnSidecarError};

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

/// Given a signed block and the cells/proofs associated with each blob in the block, assemble the
/// sidecars which can be distributed to peers.
///
/// Spec: https://github.com/ethereum/consensus-specs/blob/master/specs/fulu/validator.md#get_data_column_sidecars_from_block
pub fn get_data_column_sidecars_from_block(
    signed_block: &SignedBeaconBlock,
    cells_and_kzg_proofs: Vec<(Vec<Cell>, Vec<KZGProof>)>,
) -> Result<Vec<DataColumnSidecar>, DataColumnSidecarError> {
    let blob_kzg_commitments = signed_block.message.body.blob_kzg_commitments.clone();
    let signed_block_header = signed_block.signed_header();
    let kzg_commitments_inclusion_proof = FixedVector::new(
        signed_block
            .message
            .body
            .data_inclusion_proof(BLOB_KZG_COMMITMENTS_INDEX)
            .map_err(|err| DataColumnSidecarError::InclusionProofError(err.to_string()))?,
    )
    .map_err(|err| DataColumnSidecarError::InclusionProofError(format!("{err:?}")))?;

    get_data_column_sidecars(
        signed_block_header,
        blob_kzg_commitments,
        kzg_commitments_inclusion_proof,
        cells_and_kzg_proofs,
    )
}

#[cfg(test)]
mod tests {
    use rand::{Rng, SeedableRng, rngs::StdRng};
    use ream_consensus_misc::constants::beacon::BYTES_PER_COMMITMENT;
    use ream_execution_rpc_types::get_blobs::Blob;
    use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
    use ssz::Decode;

    use super::*;
    use crate::{
        electra::{beacon_block::BeaconBlock, beacon_block_body::BeaconBlockBody},
        matrix_entry::compute_cells_and_kzg_proofs,
    };

    const BYTES_PER_BLOB: usize = 131072;

    type CellsAndKzgProofs = Vec<(Vec<Cell>, Vec<KZGProof>)>;

    fn get_sample_blob(rng: &mut StdRng) -> Blob {
        let mut bytes = vec![0u8; BYTES_PER_BLOB];
        for chunk in bytes.chunks_mut(32) {
            rng.fill(&mut chunk[1..]);
        }
        Blob::from_ssz_bytes(&bytes).expect("constructed blob bytes should decode")
    }

    fn signed_block_with_commitments(commitments: Vec<KZGCommitment>) -> SignedBeaconBlock {
        let body = BeaconBlockBody {
            blob_kzg_commitments: VariableList::new(commitments).unwrap(),
            ..Default::default()
        };

        SignedBeaconBlock {
            message: BeaconBlock {
                body,
                ..Default::default()
            },
            signature: Default::default(),
        }
    }

    /// Builds the real KZG inputs a proposer would have: a block carrying the commitments plus the
    /// matching cells/proofs for each blob. Shared by the sidecar-assembly tests.
    fn sample_block_with_kzg(
        blob_count: usize,
    ) -> (
        DASContext,
        SignedBeaconBlock,
        Vec<KZGCommitment>,
        CellsAndKzgProofs,
    ) {
        let mut rng = StdRng::seed_from_u64(1234);
        let context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);

        let blobs: Vec<Blob> = (0..blob_count).map(|_| get_sample_blob(&mut rng)).collect();
        let commitments: Vec<KZGCommitment> = blobs
            .iter()
            .map(|blob| {
                let bytes: Vec<u8> = blob.inner.clone().into();
                let blob_array: &[u8; BYTES_PER_BLOB] = bytes.as_slice().try_into().unwrap();
                KZGCommitment(context.blob_to_kzg_commitment(blob_array).unwrap())
            })
            .collect();
        let cells_and_kzg_proofs: Vec<(Vec<Cell>, Vec<KZGProof>)> = blobs
            .iter()
            .map(|blob| compute_cells_and_kzg_proofs(blob, &context).unwrap())
            .collect();

        let signed_block = signed_block_with_commitments(commitments.clone());
        (context, signed_block, commitments, cells_and_kzg_proofs)
    }

    #[test]
    fn test_get_data_column_sidecars() {
        let (_, signed_block, _commitments, cells_and_kzg_proofs) = sample_block_with_kzg(2);

        let expected_sidecars =
            get_data_column_sidecars_from_block(&signed_block, cells_and_kzg_proofs.clone())
                .unwrap();

        let recomputed_sidecars =
            get_data_column_sidecars_from_block(&signed_block, cells_and_kzg_proofs).unwrap();

        assert_eq!(recomputed_sidecars.len(), expected_sidecars.len(),);

        assert_eq!(recomputed_sidecars, expected_sidecars,);
    }

    #[test]
    fn test_get_data_column_sidecars_from_column_sidecar() {
        let (_, signed_block, _commitments, cells_and_kzg_proofs) = sample_block_with_kzg(2);

        let sidecars =
            get_data_column_sidecars_from_block(&signed_block, cells_and_kzg_proofs.clone())
                .unwrap();

        let base_sidecar = sidecars[0].clone();

        let recomputed =
            get_data_column_sidecars_from_column_sidecar(base_sidecar, cells_and_kzg_proofs)
                .unwrap();

        assert_eq!(recomputed.len(), sidecars.len(),);

        assert_eq!(recomputed, sidecars,);
    }

    #[test]
    fn test_get_data_column_sidecars_from_block() {
        let (context, signed_block, commitments, cells_and_kzg_proofs) = sample_block_with_kzg(2);
        let blob_count = commitments.len();

        let sidecars =
            get_data_column_sidecars_from_block(&signed_block, cells_and_kzg_proofs.clone())
                .unwrap();

        assert_eq!(sidecars.len() as u64, NUMBER_OF_COLUMNS);

        let expected_header = signed_block.signed_header();

        // gather every column into one KZG batch and verify it at the end
        let mut batch_commitments: Vec<[u8; BYTES_PER_COMMITMENT]> = Vec::new();
        let mut batch_cell_indices: Vec<u64> = Vec::new();
        let mut batch_cells: Vec<[u8; 2048]> = Vec::new();
        let mut batch_proofs: Vec<[u8; BYTES_PER_COMMITMENT]> = Vec::new();

        for (column_index, sidecar) in sidecars.iter().enumerate() {
            assert_eq!(sidecar.index, column_index as u64);
            assert_eq!(
                sidecar.kzg_commitments,
                signed_block.message.body.blob_kzg_commitments
            );
            assert_eq!(sidecar.signed_block_header, expected_header);
            assert_eq!(sidecar.column.len(), blob_count);
            assert_eq!(sidecar.kzg_proofs.len(), blob_count);
            assert!(sidecar.verify_inclusion_proof());

            for (blob_index, (cells, proofs)) in cells_and_kzg_proofs.iter().enumerate() {
                // each cell/proof should sit in its column slot
                assert_eq!(sidecar.column[blob_index], cells[column_index]);
                assert_eq!(sidecar.kzg_proofs[blob_index], proofs[column_index]);

                batch_commitments.push(commitments[blob_index].0);
                batch_cell_indices.push(sidecar.index);
                batch_cells.push(sidecar.column[blob_index].as_ref().try_into().unwrap());
                batch_proofs.push(sidecar.kzg_proofs[blob_index].0);
            }
        }

        context
            .verify_cell_kzg_proof_batch(
                batch_commitments.iter().collect(),
                batch_cell_indices,
                batch_cells.iter().collect(),
                batch_proofs.iter().collect(),
            )
            .expect("assembled sidecars should pass KZG verification");
    }

    #[test]
    fn test_get_data_column_sidecars_from_block_count_mismatch() {
        let signed_block = signed_block_with_commitments(vec![
            KZGCommitment([0u8; BYTES_PER_COMMITMENT]),
            KZGCommitment([1u8; BYTES_PER_COMMITMENT]),
        ]); // two commitments
        let cells_and_proofs = vec![(
            vec![Cell::default(); NUMBER_OF_COLUMNS as usize],
            vec![KZGProof::default(); NUMBER_OF_COLUMNS as usize],
        )]; // only one cell with 1 proof to make it fail

        let result = get_data_column_sidecars_from_block(&signed_block, cells_and_proofs);
        assert!(matches!(
            result,
            Err(DataColumnSidecarError::CommitmentCountMismatch { .. })
        ));
    }

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
