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

    /// Cheap structural checks, mirroring `DataColumnSidecar::verify()` but kept
    /// separate on purpose: it returns typed `ValidationError`s (so rejections
    /// can be counted and diagnosed) instead of a `bool`,
    fn check_shape(&self, sidecar: &DataColumnSidecar) -> Result<(), ValidationError> {
        // Cells, commitments, and proofs are one-per-blob, so their counts must
        // agree and stay within the per-block blob limit.
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

        // Slot consistency
        if candidate.context.slot != sidecar.signed_block_header.message.slot {
            return Err(ValidationError::SlotMismatch {
                expected: sidecar.signed_block_header.message.slot,
                actual: candidate.context.slot,
            });
        }

        // Structural well-formedness (counts, blob limit) before the costly proofs.
        self.check_shape(&sidecar)?;

        // The commitments really belong to this block: a Merkle inclusion proof
        // against the signed header's body root. The sidecar checks this itself,
        // so the adapter never reimplements the Merkle branch.
        if !sidecar.verify_inclusion_proof() {
            return Err(ValidationError::InvalidInclusionProof);
        }

        // The scheme-specific cryptographic step.
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
        column::{CandidateColumn, DaContext, DaPayload},
        error::ValidationError,
        id::DaColumnId,
        verifier::DaVerifier,
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
    fn accepts_a_valid_sidecar() {
        // Real KZG: an all-zero blob → 128 cells + 128 cell proofs.
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

        // Self-built inclusion proof: the DA node doesn't verify block
        // authenticity, so `body_root` can be a synthetic 16-leaf tree whose
        // `BLOB_KZG_COMMITMENTS_INDEX` leaf is the commitments root — the branch
        // then verifies without needing a real block body.
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

        // One real column, all the way through the verifier: it must be accepted.
        let sidecar = &sidecars[7];
        let verified = verifier()
            .verify(candidate_of(sidecar))
            .expect("a KZG-valid sidecar is accepted");

        assert_eq!(verified.id().index(), sidecar.index);
        assert_eq!(verified.payload().as_bytes(), sidecar.as_ssz_bytes());
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
    fn rejects_slot_mismatch() {
        let sidecar = sidecar(3, 1);
        let honest = candidate_of(&sidecar);
        // Same payload, but the envelope claims a different slot than the
        // sidecar's own header — which would mis-bucket it for retention.
        let forged = CandidateColumn {
            context: DaContext {
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

    /// `ream-da` copies `NUMBER_OF_COLUMNS` so the core stays free of the beacon
    /// crate; beacon keeps its own copy for sidecar logic. Neither may depend on
    /// the other, so the two definitions cannot be unified in a type. This
    /// adapter is the one crate that sees both, making it the only place that can
    /// pin them equal — turning `id.rs`'s "MUST stay equal" comment into a
    /// CI-enforced invariant that trips the moment the two drift apart.
    #[test]
    fn da_core_column_count_matches_beacon() {
        assert_eq!(
            ream_da::id::NUMBER_OF_COLUMNS,
            ream_consensus_beacon::data_column_sidecar::NUMBER_OF_COLUMNS,
        );
    }
}
