use std::num::NonZeroUsize;

use ream_consensus_beacon::data_column_sidecar::DataColumnSidecar;
use ream_da::{
    column::{CandidateColumn, VerifiedColumn},
    error::ValidationError,
    id::ColumnId,
    verifier::ColumnVerifier,
};
use ream_polynomial_commitments::{handlers::verify_data_column_sidecar_kzg_proofs, trusted_setup};
use ssz::Decode;
use tree_hash::TreeHash;

/// Decodes a candidate's payload as an SSZ `DataColumnSidecar` and admits it
/// only if it is structurally sound and its cells verify against their KZG
/// commitments.
#[derive(Debug, Clone, Copy)]
pub struct KzgVerifier {
    /// Per-block blob limit; `NonZeroUsize` makes a zero limit — which would
    /// reject every column — unrepresentable.
    max_blobs_per_block: NonZeroUsize,
}

impl KzgVerifier {
    pub fn new(max_blobs_per_block: NonZeroUsize) -> Self {
        Self {
            max_blobs_per_block,
        }
    }

    /// Eagerly load the KZG trusted setup (a one-time, multi-second cost);
    /// call at startup so the first column doesn't pay it mid-request.
    pub fn warm_up_trusted_setup() {
        let _ = trusted_setup::blst_settings();
    }

    fn decode(&self, bytes: &[u8]) -> Result<DataColumnSidecar, ValidationError> {
        DataColumnSidecar::from_ssz_bytes(bytes)
            .map_err(|err| ValidationError::MalformedPayload(format!("{err:?}")))
    }

    /// Mirrors `DataColumnSidecar::verify()`, kept separate to return typed
    /// `ValidationError`s instead of a `bool`.
    fn check_shape(&self, sidecar: &DataColumnSidecar) -> Result<(), ValidationError> {
        let commitments = sidecar.kzg_commitments.len();
        if commitments == 0 {
            return Err(ValidationError::EmptyCommitments);
        }
        if commitments > self.max_blobs_per_block.get() {
            return Err(ValidationError::TooManyCommitments {
                count: commitments,
                maximum: self.max_blobs_per_block.get(),
            });
        }
        if sidecar.column.len() != commitments || sidecar.kzg_proofs.len() != commitments {
            return Err(ValidationError::LengthMismatch {
                cells: sidecar.column.len(),
                commitments,
                proofs: sidecar.kzg_proofs.len(),
            });
        }
        Ok(())
    }
}

impl ColumnVerifier for KzgVerifier {
    fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError> {
        let sidecar = self.decode(&candidate.payload)?;

        // The id is derived from the payload's own signed header, so a
        // candidate cannot claim a column its payload does not carry.
        let block_root = sidecar.signed_block_header.message.tree_hash_root();
        let id = ColumnId::new(block_root, sidecar.index)?;
        if id != candidate.id {
            return Err(ValidationError::IdMismatch {
                expected: format!("block root {block_root}, column {}", sidecar.index),
                actual: format!(
                    "block root {}, column {}",
                    candidate.id.block_root(),
                    candidate.id.index()
                ),
            });
        }

        if candidate.context.slot != sidecar.signed_block_header.message.slot {
            return Err(ValidationError::SlotMismatch {
                expected: sidecar.signed_block_header.message.slot,
                actual: candidate.context.slot,
            });
        }

        // Cheap structural checks before the costly proofs.
        self.check_shape(&sidecar)?;

        // The inclusion proof binds the commitments to the signed header's
        // body root.
        if !sidecar.verify_inclusion_proof() {
            return Err(ValidationError::InvalidInclusionProof);
        }

        match verify_data_column_sidecar_kzg_proofs(&sidecar) {
            Ok(true) => {}
            Ok(false) => return Err(ValidationError::InvalidProof),
            Err(err) => return Err(ValidationError::VerifierFailure(format!("{err:?}"))),
        }

        Ok(VerifiedColumn::new_unchecked(
            candidate.id,
            candidate.context,
            candidate.payload,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use alloy_primitives::B256;
    use ream_consensus_beacon::{
        data_column_sidecar::{Cell, DataColumnSidecar, get_data_column_sidecars},
        matrix_entry::compute_cells_and_kzg_proofs,
    };
    use ream_consensus_misc::{
        beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader},
        constants::beacon::{BLOB_KZG_COMMITMENTS_INDEX, DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH},
        polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
    };
    use ream_da::{
        column::{CandidateColumn, ColumnContext},
        error::ValidationError,
        id::ColumnId,
        verifier::ColumnVerifier,
    };
    use ream_execution_rpc_types::get_blobs::Blob;
    use ream_merkle::{generate_proof, merkle_tree};
    use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
    use ssz::Encode;
    use ssz_types::{FixedVector, VariableList};
    use tree_hash::TreeHash;

    use super::KzgVerifier;

    const MAX_BLOBS: usize = 9;

    fn verifier() -> KzgVerifier {
        KzgVerifier::new(NonZeroUsize::new(MAX_BLOBS).expect("nonzero"))
    }

    /// A well-formed sidecar whose zeroed inclusion proof never verifies —
    /// fine for exercising the cheaper reject paths that run before it.
    fn sidecar(index: u64, blobs: usize) -> DataColumnSidecar {
        DataColumnSidecar {
            index,
            column: VariableList::new(vec![Cell::default(); blobs]).expect("column within bounds"),
            kzg_commitments: VariableList::new(vec![KZGCommitment::empty_for_testing(); blobs])
                .expect("commitments within bounds"),
            kzg_proofs: VariableList::new(vec![KZGProof::default(); blobs])
                .expect("proofs within bounds"),
            signed_block_header: SignedBeaconBlockHeader::default(),
            kzg_commitments_inclusion_proof: FixedVector::default(),
        }
    }

    fn payload_of(sidecar: &DataColumnSidecar) -> Vec<u8> {
        sidecar.as_ssz_bytes()
    }

    fn candidate_of(sidecar: &DataColumnSidecar) -> CandidateColumn {
        let block_root = sidecar.signed_block_header.message.tree_hash_root();
        CandidateColumn {
            id: ColumnId::new(block_root, sidecar.index).expect("valid index"),
            context: ColumnContext {
                slot: sidecar.signed_block_header.message.slot,
            },
            payload: payload_of(sidecar),
        }
    }

    #[test]
    fn accepts_a_valid_sidecar() {
        let blob = Blob {
            inner: FixedVector::default(),
        };
        let das_context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);
        let (cells, proofs) =
            compute_cells_and_kzg_proofs(&blob, &das_context).expect("compute cells and proofs");

        // The zero polynomial's commitment is the G1 point at infinity.
        let mut commitment_bytes = [0u8; 48];
        commitment_bytes[0] = 0xc0;
        let kzg_commitments = VariableList::new(vec![KZGCommitment(commitment_bytes)])
            .expect("one commitment within bounds");

        // `body_root` is a synthetic tree whose `BLOB_KZG_COMMITMENTS_INDEX`
        // leaf is the commitments root, so the branch verifies without a real
        // block body.
        let mut leaves = vec![B256::ZERO; 1 << DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH];
        leaves[BLOB_KZG_COMMITMENTS_INDEX as usize] = kzg_commitments.tree_hash_root();
        let tree = merkle_tree(&leaves, DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH).expect("merkle tree");
        let inclusion_proof = FixedVector::new(
            generate_proof(
                &tree,
                BLOB_KZG_COMMITMENTS_INDEX,
                DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH,
            )
            .expect("inclusion proof"),
        )
        .expect("proof length matches depth");

        let signed_block_header = SignedBeaconBlockHeader {
            message: BeaconBlockHeader {
                slot: 1,
                proposer_index: 0,
                parent_root: B256::ZERO,
                state_root: B256::ZERO,
                body_root: tree[1],
            },
            signature: Default::default(),
        };

        let sidecars = get_data_column_sidecars(
            signed_block_header,
            kzg_commitments,
            inclusion_proof,
            vec![(cells, proofs)],
        )
        .expect("assemble column sidecars");

        let sidecar = &sidecars[7];
        let verified = verifier()
            .verify(candidate_of(sidecar))
            .expect("a KZG-valid sidecar is accepted");

        assert_eq!(verified.id().index(), sidecar.index);
        assert_eq!(verified.payload(), sidecar.as_ssz_bytes());
    }

    #[test]
    fn rejects_malformed_payload() {
        let candidate = CandidateColumn {
            id: ColumnId::new(B256::ZERO, 0).expect("valid index"),
            context: ColumnContext::default(),
            payload: vec![0xde, 0xad, 0xbe, 0xef],
        };
        assert!(matches!(
            verifier().verify(candidate),
            Err(ValidationError::MalformedPayload(_))
        ));
    }

    #[test]
    fn rejects_out_of_range_index() {
        // index 128 == NUMBER_OF_COLUMNS, never valid.
        let candidate = CandidateColumn {
            id: ColumnId::new(B256::ZERO, 0).expect("valid index"),
            context: ColumnContext::default(),
            payload: payload_of(&sidecar(128, 1)),
        };
        assert!(matches!(
            verifier().verify(candidate),
            Err(ValidationError::InvalidColumnIndex { .. })
        ));
    }

    #[test]
    fn rejects_id_mismatch() {
        let sidecar = sidecar(3, 1);
        let honest = candidate_of(&sidecar);
        let forged = CandidateColumn {
            id: ColumnId::new(honest.id.block_root(), 4).expect("valid index"),
            ..honest
        };
        assert!(matches!(
            verifier().verify(forged),
            Err(ValidationError::IdMismatch { .. })
        ));
    }

    #[test]
    fn rejects_slot_mismatch() {
        let sidecar = sidecar(3, 1);
        let honest = candidate_of(&sidecar);
        let forged = CandidateColumn {
            context: ColumnContext {
                slot: honest.context.slot + 1,
            },
            ..honest
        };
        assert!(matches!(
            verifier().verify(forged),
            Err(ValidationError::SlotMismatch { .. })
        ));
    }

    #[test]
    fn rejects_empty_commitments() {
        assert!(matches!(
            verifier().verify(candidate_of(&sidecar(0, 0))),
            Err(ValidationError::EmptyCommitments)
        ));
    }

    #[test]
    fn rejects_too_many_commitments() {
        assert!(matches!(
            verifier().verify(candidate_of(&sidecar(0, MAX_BLOBS + 1))),
            Err(ValidationError::TooManyCommitments { .. })
        ));
    }

    #[test]
    fn rejects_length_mismatch() {
        let mut sidecar = sidecar(0, 2);
        sidecar.kzg_proofs =
            VariableList::new(vec![KZGProof::default(); 1]).expect("proofs within bounds");
        assert!(matches!(
            verifier().verify(candidate_of(&sidecar)),
            Err(ValidationError::LengthMismatch { .. })
        ));
    }

    #[test]
    fn rejects_invalid_inclusion_proof() {
        assert!(matches!(
            verifier().verify(candidate_of(&sidecar(0, 1))),
            Err(ValidationError::InvalidInclusionProof)
        ));
    }

    /// `ream-da` and beacon each define `NUMBER_OF_COLUMNS` and neither may
    /// depend on the other; this adapter sees both, so it pins them equal.
    #[test]
    fn das_core_column_count_matches_beacon() {
        assert_eq!(
            ream_da::id::NUMBER_OF_COLUMNS,
            ream_consensus_beacon::data_column_sidecar::NUMBER_OF_COLUMNS,
        );
    }
}
