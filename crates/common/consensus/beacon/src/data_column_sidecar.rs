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

pub type Cell = FixedVector<u8, typenum::U2048>;

pub const NUMBER_OF_COLUMNS: u64 = 128;
pub const DATA_COLUMN_SIDECAR_SUBNET_COUNT: u64 = 128;

pub type MaxBlobCommitmentsPerBlock = typenum::U6;

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
