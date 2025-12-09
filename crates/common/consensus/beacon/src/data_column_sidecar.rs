use alloy_primitives::B256;
use ream_consensus_misc::beacon_block_header::SignedBeaconBlockHeader;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, VariableList, typenum};
use tree_hash_derive::TreeHash;

use crate::polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof};

pub type Cell = FixedVector<u8, typenum::U2048>;
pub type ColumnIndex = u64;

pub const NUMBER_OF_COLUMNS: u64 = 128;
pub const DATA_COLUMN_SIDECAR_SUBNET_COUNT: u64 = 128;

pub type MaxBlobCommitmentsPerBlock = typenum::U6;

pub type KzgCommitmentInclusionProofDepth = typenum::U17;

// TODO remove this and use from PR of issue 1038
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Encode, Decode, TreeHash)]
pub struct DataColumnSidecar {
    pub index: ColumnIndex,
    pub column: VariableList<Cell, MaxBlobCommitmentsPerBlock>,
    pub kzg_commitments: VariableList<KZGCommitment, MaxBlobCommitmentsPerBlock>,
    pub kzg_proofs: VariableList<KZGProof, MaxBlobCommitmentsPerBlock>,
    pub signed_block_header: SignedBeaconBlockHeader,
    pub kzg_commitments_inclusion_proof: FixedVector<B256, KzgCommitmentInclusionProofDepth>,
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
    pub index: ColumnIndex,
}

impl ColumnIdentifier {
    pub fn new(block_root: B256, index: ColumnIndex) -> Self {
        Self { block_root, index }
    }
}

impl DataColumnSidecar {
    pub fn compute_subnet(&self) -> u64 {
        self.index % DATA_COLUMN_SIDECAR_SUBNET_COUNT
    }

    pub fn verify_inclusion_proof(&self) -> bool {
        // TODO needs verification
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
