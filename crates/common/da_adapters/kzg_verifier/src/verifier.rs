use std::num::NonZeroUsize;

use ream_consensus_beacon::data_column_sidecar::DataColumnSidecar;
use ream_da::{
    column::{CandidateColumn, VerifiedColumn},
    error::ValidationError,
    id::DaColumnId,
    verifier::DaVerifier,
};
use ream_polynomial_commitments::{handlers::verify_data_column_sidecar_kzg_proofs, trusted_setup};
use ssz::Decode;
use tree_hash::TreeHash;

/// PeerDAS verifier: reads a candidate's opaque payload as an SSZ
/// `DataColumnSidecar` and admits it only if it is structurally sound and its
/// cells verify against their KZG commitments.
#[derive(Debug, Clone, Copy)]
pub struct KzgVerifier {
    /// Upper bound on commitments (blobs) per block. `NonZeroUsize` makes a limit
    /// of zero — which would reject every column — unrepresentable.
    max_blobs_per_block: NonZeroUsize,
}

impl KzgVerifier {
    pub fn new(max_blobs_per_block: NonZeroUsize) -> Self {
        Self {
            max_blobs_per_block,
        }
    }

    /// Eagerly load the KZG trusted setup, which is otherwise lazily initialized
    /// on first use at a one-time, multi-second cost. Call once at startup, off
    /// the hot path, so the first column to arrive doesn't pay it mid-request.
    pub fn warm_up_trusted_setup() {
        let _ = trusted_setup::blst_settings();
    }

    /// Decode the opaque payload into a PeerDAS column sidecar.
    fn decode(&self, bytes: &[u8]) -> Result<DataColumnSidecar, ValidationError> {
        DataColumnSidecar::from_ssz_bytes(bytes)
            .map_err(|err| ValidationError::MalformedPayload(format!("{err:?}")))
    }

    /// The one cryptographically scheme-specific step: every cell matches its
    /// commitment and proof.
    fn verify_cells(&self, sidecar: &DataColumnSidecar) -> anyhow::Result<bool> {
        verify_data_column_sidecar_kzg_proofs(sidecar)
    }
}

impl DaVerifier for KzgVerifier {
    fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError> {
        let sidecar = self.decode(candidate.payload.as_bytes())?;

        // Identifier consistency: the id is derived from the sidecar's own signed
        // header, so a candidate cannot claim a (block root, column) its payload
        // does not actually carry.
        let block_root = sidecar.signed_block_header.message.tree_hash_root();
        let id = DaColumnId::new(block_root, sidecar.index)?;
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

        // Cheap shape checks before the expensive proof work. Cells, commitments,
        // and proofs are one-per-blob, so their counts must agree and stay within
        // the per-block blob limit.
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

        // The commitments really belong to this block: a Merkle inclusion proof
        // against the signed header's body root. The sidecar checks this itself,
        // so the adapter never reimplements the Merkle branch.
        if !sidecar.verify_inclusion_proof() {
            return Err(ValidationError::InvalidInclusionProof);
        }

        // The scheme-specific cryptographic step.
        match self.verify_cells(&sidecar) {
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
    use ream_consensus_beacon::data_column_sidecar::{Cell, DataColumnSidecar};
    use ream_consensus_misc::{
        beacon_block_header::SignedBeaconBlockHeader,
        polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
    };
    use ream_da::{
        column::{CandidateColumn, DaContext, DaPayload},
        error::ValidationError,
        id::DaColumnId,
        verifier::DaVerifier,
    };
    use ssz::Encode;
    use ssz_types::{FixedVector, VariableList};
    use tree_hash::TreeHash;

    use super::KzgVerifier;

    const MAX_BLOBS: usize = 9;

    fn verifier() -> KzgVerifier {
        KzgVerifier::new(NonZeroUsize::new(MAX_BLOBS).expect("nonzero"))
    }

    /// A structurally well-formed sidecar with `blobs` cells/commitments/proofs.
    /// Its inclusion proof is zeroed, so it never passes `verify_inclusion_proof`
    /// — which is fine for exercising the cheaper reject paths that run before it.
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

    fn payload_of(sidecar: &DataColumnSidecar) -> DaPayload {
        DaPayload::new(sidecar.as_ssz_bytes())
    }

    /// An honest candidate whose id is derived from the sidecar's own header.
    fn candidate_of(sidecar: &DataColumnSidecar) -> CandidateColumn {
        let block_root = sidecar.signed_block_header.message.tree_hash_root();
        CandidateColumn {
            id: DaColumnId::new(block_root, sidecar.index).expect("valid index"),
            context: DaContext {
                slot: sidecar.signed_block_header.message.slot,
            },
            payload: payload_of(sidecar),
        }
    }

    #[test]
    fn rejects_malformed_payload() {
        let candidate = CandidateColumn {
            id: DaColumnId::new(B256::ZERO, 0).expect("valid index"),
            context: DaContext::default(),
            payload: DaPayload::new(vec![0xde, 0xad, 0xbe, 0xef]),
        };
        assert!(matches!(
            verifier().verify(candidate),
            Err(ValidationError::MalformedPayload(_))
        ));
    }

    #[test]
    fn rejects_out_of_range_index() {
        // index 128 == NUMBER_OF_COLUMNS: the id derived from the decoded sidecar
        // is itself invalid, rejected before any id comparison.
        let candidate = CandidateColumn {
            id: DaColumnId::new(B256::ZERO, 0).expect("valid index"),
            context: DaContext::default(),
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
        // Same payload, but the envelope claims a different column.
        let forged = CandidateColumn {
            id: DaColumnId::new(honest.id.block_root(), 4).expect("valid index"),
            ..honest
        };
        assert!(matches!(
            verifier().verify(forged),
            Err(ValidationError::IdMismatch { .. })
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
        // Drop one proof so cells/commitments/proofs no longer agree.
        sidecar.kzg_proofs =
            VariableList::new(vec![KZGProof::default(); 1]).expect("proofs within bounds");
        assert!(matches!(
            verifier().verify(candidate_of(&sidecar)),
            Err(ValidationError::LengthMismatch { .. })
        ));
    }

    #[test]
    fn rejects_invalid_inclusion_proof() {
        // Well-formed shape, but the zeroed inclusion proof cannot match the
        // header's body root, so it fails right before the KZG cell check.
        assert!(matches!(
            verifier().verify(candidate_of(&sidecar(0, 1))),
            Err(ValidationError::InvalidInclusionProof)
        ));
    }
}
