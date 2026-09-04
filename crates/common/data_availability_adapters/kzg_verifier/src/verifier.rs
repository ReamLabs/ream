use std::{num::NonZeroUsize, sync::OnceLock};

use ream_consensus_beacon::{
    data_column_sidecar::{DataColumnSidecar, get_data_column_sidecars_from_column_sidecar},
    matrix_entry::recover_cells_and_kzg_proofs,
};
use ream_consensus_misc::{blob_parameters::BlobParameters, misc::compute_epoch_at_slot};
use ream_data_availability::{
    column::{CandidateBlock, CandidateColumn, VerifiedColumn},
    error::ValidationError,
    id::{ColumnId, NUMBER_OF_COLUMNS},
    reconstruction::ColumnReconstructor,
    verifier::ColumnVerifier,
};
use ream_polynomial_commitments::{
    handlers::{verify_data_column_sidecar_kzg_proofs, verify_data_column_sidecars_batch},
    trusted_setup,
};
use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
use ssz::{Decode, Encode};
use tree_hash::TreeHash;

/// The PeerDAS adapter for the DA core's cryptographic capabilities — one type
/// with two roles sharing one trusted setup.
#[derive(Debug, Clone)]
pub struct KzgAdapter {
    /// BPO schedule `(activation epoch, blob limit)`, ascending; zero-limit
    /// entries — which would reject every column — are dropped at construction.
    blob_schedule: Vec<(u64, NonZeroUsize)>,
    max_blobs_per_block_electra: NonZeroUsize,
}

impl KzgAdapter {
    pub fn new(
        blob_schedule: impl IntoIterator<Item = BlobParameters>,
        max_blobs_per_block_electra: NonZeroUsize,
    ) -> Self {
        let mut blob_schedule: Vec<(u64, NonZeroUsize)> = blob_schedule
            .into_iter()
            .filter_map(|entry| {
                NonZeroUsize::new(entry.max_blobs_per_block as usize)
                    .map(|limit| (entry.epoch, limit))
            })
            .collect();
        blob_schedule.sort_by_key(|(epoch, _)| *epoch);
        Self {
            blob_schedule,
            max_blobs_per_block_electra,
        }
    }

    /// The blob limit in force at `epoch`: the newest schedule entry activated
    /// at or before it, falling back to the Electra limit. Mirrors the spec's
    /// `get_blob_parameters`.
    fn max_blobs_at(&self, epoch: u64) -> NonZeroUsize {
        self.blob_schedule
            .iter()
            .rev()
            .find(|(activation_epoch, _)| epoch >= *activation_epoch)
            .map(|(_, limit)| *limit)
            .unwrap_or(self.max_blobs_per_block_electra)
    }

    /// Eagerly load the KZG trusted setup (a one-time, multi-second cost);
    /// call at startup so the first column doesn't pay it mid-request.
    pub fn warm_up_trusted_setup() {
        let _ = trusted_setup::blst_settings();
    }

    /// The cell-recovery context, built from the trusted setup once and cached
    /// for the process lifetime.
    fn recovery_context() -> &'static DASContext {
        static CONTEXT: OnceLock<DASContext> = OnceLock::new();
        CONTEXT.get_or_init(|| DASContext::new(&TrustedSetup::default(), UsePrecomp::No))
    }

    fn decode(&self, bytes: &[u8]) -> Result<DataColumnSidecar, ValidationError> {
        DataColumnSidecar::from_ssz_bytes(bytes)
            .map_err(|err| ValidationError::MalformedPayload(format!("{err:?}")))
    }

    /// Mirrors `DataColumnSidecar::verify()`, kept separate to return typed
    /// `ValidationError`s instead of a `bool`.
    fn check_shape(&self, sidecar: &DataColumnSidecar) -> Result<(), ValidationError> {
        let epoch = compute_epoch_at_slot(sidecar.signed_block_header.message.slot);
        let max_blobs = self.max_blobs_at(epoch).get();
        let commitments = sidecar.kzg_commitments.len();
        if commitments == 0 {
            return Err(ValidationError::EmptyCommitments);
        }
        if commitments > max_blobs {
            return Err(ValidationError::TooManyCommitments {
                count: commitments,
                maximum: max_blobs,
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

    /// Every check before the kzg verification
    fn precheck(&self, candidate: &CandidateColumn) -> Result<DataColumnSidecar, ValidationError> {
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

        Ok(sidecar)
    }
}

impl ColumnVerifier for KzgAdapter {
    fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError> {
        let sidecar = self.precheck(&candidate)?;

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

    /// Batched verification of a whole block's columns.
    fn verify_block(
        &self,
        block: CandidateBlock,
    ) -> Vec<(u64, Result<VerifiedColumn, ValidationError>)> {
        // Step 1: prechecks, one verdict slot per column in submission order.
        let mut verdicts: Vec<(u64, Result<VerifiedColumn, ValidationError>)> = Vec::new();
        let mut survivors: Vec<(usize, DataColumnSidecar, CandidateColumn)> = Vec::new();
        for candidate in block.into_columns() {
            let index = candidate.id.index();
            match self.precheck(&candidate) {
                Ok(sidecar) => {
                    survivors.push((verdicts.len(), sidecar, candidate));
                    // To keep the order of index, these values will be overwritten in the step 2
                    verdicts.push((index, Err(ValidationError::InvalidProof)));
                }
                Err(err) => verdicts.push((index, Err(err))),
            }
        }
        if survivors.is_empty() {
            return verdicts;
        }

        // Step 2: one cross-column KZG batch over every survivor.
        let batch_verdict =
            verify_data_column_sidecars_batch(survivors.iter().map(|(_, sidecar, _)| sidecar));
        match batch_verdict {
            Ok(true) => {
                for (position, _, candidate) in survivors {
                    verdicts[position].1 = Ok(VerifiedColumn::new_unchecked(
                        candidate.id,
                        candidate.context,
                        candidate.payload,
                    ));
                }
            }
            // Fallback: if batch verifcation fails, re-check per column
            Ok(false) | Err(_) => {
                for (position, sidecar, candidate) in survivors {
                    verdicts[position].1 = match verify_data_column_sidecar_kzg_proofs(&sidecar) {
                        Ok(true) => Ok(VerifiedColumn::new_unchecked(
                            candidate.id,
                            candidate.context,
                            candidate.payload,
                        )),
                        Ok(false) => Err(ValidationError::InvalidProof),
                        Err(err) => Err(ValidationError::VerifierFailure(format!("{err:?}"))),
                    };
                }
            }
        }
        verdicts
    }
}

impl ColumnReconstructor for KzgAdapter {
    /// PeerDAS recovery
    fn reconstruct(
        &self,
        held: Vec<VerifiedColumn>,
    ) -> Result<Vec<CandidateColumn>, ValidationError> {
        let first = held.first().ok_or(ValidationError::EmptyBatch)?;
        let block_root = first.id().block_root();
        let context = first.context();

        // Decode every held sidecar. All must belong to one block and agree on
        // the blob count, or the row transposition below is meaningless.
        let mut sidecars = Vec::with_capacity(held.len());
        let mut held_mask = 0u128;
        for column in &held {
            if column.id().block_root() != block_root {
                return Err(ValidationError::ReconstructionFailure(format!(
                    "mixed blocks in one recovery: {block_root} and {}",
                    column.id().block_root()
                )));
            }
            let sidecar = self.decode(column.payload())?;
            if sidecar.index != column.id().index() {
                return Err(ValidationError::ReconstructionFailure(format!(
                    "column {} carries a sidecar for index {}",
                    column.id().index(),
                    sidecar.index
                )));
            }
            held_mask |= 1u128 << column.id().index();
            sidecars.push(sidecar);
        }
        let blob_count = sidecars[0].kzg_commitments.len();
        for sidecar in &sidecars {
            if sidecar.column.len() != blob_count || sidecar.kzg_commitments.len() != blob_count {
                return Err(ValidationError::ReconstructionFailure(format!(
                    "column {} disagrees on the block's blob count",
                    sidecar.index
                )));
            }
        }
        if u64::from(held_mask.count_ones()) < NUMBER_OF_COLUMNS / 2 {
            return Err(ValidationError::ReconstructionFailure(format!(
                "{} distinct columns held, recovery needs at least {}",
                held_mask.count_ones(),
                NUMBER_OF_COLUMNS / 2
            )));
        }

        // Row-by-row recovery: a column holds cell `column index` of every
        // blob row, so the held cell indices are the same for every row.
        let cell_indices: Vec<u64> = sidecars.iter().map(|sidecar| sidecar.index).collect();
        let mut cells_and_proofs = Vec::with_capacity(blob_count);
        let das_context = Self::recovery_context();
        for row in 0..blob_count {
            let cells: Vec<_> = sidecars
                .iter()
                .map(|sidecar| sidecar.column[row].clone())
                .collect();
            let recovered = recover_cells_and_kzg_proofs(cell_indices.clone(), cells, das_context)
                .map_err(|err| {
                    ValidationError::ReconstructionFailure(format!("blob row {row}: {err:?}"))
                })?;
            cells_and_proofs.push(recovered);
        }

        // Reassemble full sidecars
        let template = sidecars.into_iter().next().expect("held is non-empty");
        let all_columns = get_data_column_sidecars_from_column_sidecar(template, cells_and_proofs)
            .map_err(|err| ValidationError::ReconstructionFailure(format!("{err:?}")))?;

        let mut recovered = Vec::new();
        for sidecar in all_columns {
            if held_mask & (1u128 << sidecar.index) != 0 {
                continue;
            }
            recovered.push(CandidateColumn {
                id: ColumnId::new(block_root, sidecar.index)?,
                context,
                payload: sidecar.as_ssz_bytes(),
            });
        }
        Ok(recovered)
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
        blob_parameters::BlobParameters,
        constants::beacon::{
            BLOB_KZG_COMMITMENTS_INDEX, DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH, SLOTS_PER_EPOCH,
        },
        polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
    };
    use ream_data_availability::{
        column::{CandidateBlock, CandidateColumn, ColumnContext, VerifiedColumn},
        error::ValidationError,
        id::ColumnId,
        reconstruction::ColumnReconstructor,
        verifier::ColumnVerifier,
    };
    use ream_execution_rpc_types::get_blobs::Blob;
    use ream_merkle::{generate_proof, merkle_tree};
    use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
    use ssz::Encode;
    use ssz_types::{FixedVector, VariableList};
    use tree_hash::TreeHash;

    use super::KzgAdapter;

    const MAX_BLOBS: usize = 9;

    /// A verifier with no BPO schedule: every epoch uses the Electra fallback,
    /// so tests that don't care about the schedule see one fixed limit.
    fn verifier() -> KzgAdapter {
        KzgAdapter::new([], NonZeroUsize::new(MAX_BLOBS).expect("nonzero"))
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

    /// A full set of 128 KZG-valid sidecars over one all-zero blob
    fn real_sidecars() -> Vec<DataColumnSidecar> {
        let blob = Blob {
            inner: FixedVector::default(),
        };
        let das_context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);
        let (cells, proofs) =
            compute_cells_and_kzg_proofs(&blob, &das_context).expect("compute cells and proofs");

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

        get_data_column_sidecars(
            signed_block_header,
            kzg_commitments,
            inclusion_proof,
            vec![(cells, proofs)],
        )
        .expect("assemble column sidecars")
    }

    fn block_of(sidecars: &[DataColumnSidecar]) -> CandidateBlock {
        let header = &sidecars[0].signed_block_header.message;
        CandidateBlock::new(
            header.tree_hash_root(),
            ColumnContext { slot: header.slot },
            sidecars
                .iter()
                .map(|sidecar| (sidecar.index, payload_of(sidecar)))
                .collect(),
        )
        .expect("a valid batch")
    }

    #[test]
    fn accepts_a_valid_sidecar() {
        let sidecars = real_sidecars();

        let sidecar = &sidecars[7];
        let verified = verifier()
            .verify(candidate_of(sidecar))
            .expect("a KZG-valid sidecar is accepted");

        assert_eq!(verified.id().index(), sidecar.index);
        assert_eq!(verified.payload(), sidecar.as_ssz_bytes());
    }

    #[test]
    fn verify_block_accepts_a_real_batch() {
        let sidecars = real_sidecars();

        // Several real columns as one batch: every verdict comes back Ok
        // through the single cross-column KZG check.
        let verdicts = verifier().verify_block(block_of(&sidecars[0..4]));

        assert_eq!(verdicts.len(), 4);
        for (position, (index, verdict)) in verdicts.iter().enumerate() {
            assert_eq!(*index, position as u64);
            let verified = verdict.as_ref().expect("a KZG-valid column is accepted");
            assert_eq!(verified.id().index(), *index);
        }
    }

    #[test]
    fn verify_block_corrupt_one_proof() {
        let sidecars = real_sidecars();

        // Corrupt one column's proof: the batched check fails as a whole, and
        // the per-column fallback must pin the failure on exactly that column.
        let mut batch = sidecars[0..4].to_vec();
        let mut bad_proof = batch[2].kzg_proofs[0];
        bad_proof.0[10] ^= 0xff;
        batch[2].kzg_proofs = VariableList::new(vec![bad_proof]).expect("proofs within bounds");

        let verdicts = verifier().verify_block(block_of(&batch));

        assert_eq!(verdicts.len(), 4);
        for (index, verdict) in &verdicts {
            match index {
                2 => assert!(
                    matches!(
                        verdict,
                        Err(ValidationError::InvalidProof)
                            | Err(ValidationError::VerifierFailure(_))
                    ),
                    "the corrupted column is rejected"
                ),
                _ => assert!(verdict.is_ok(), "innocent column {index} still passes"),
            }
        }
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
    fn blob_schedule_governs_the_limit_per_epoch() {
        let verifier = KzgAdapter::new(
            [
                BlobParameters {
                    epoch: 2,
                    max_blobs_per_block: 15,
                },
                BlobParameters {
                    epoch: 4,
                    max_blobs_per_block: 21,
                },
            ],
            NonZeroUsize::new(MAX_BLOBS).expect("nonzero"),
        );

        // Before any entry activates, the Electra fallback is in force.
        assert_eq!(verifier.max_blobs_at(0).get(), MAX_BLOBS);
        assert_eq!(verifier.max_blobs_at(1).get(), MAX_BLOBS);
        // Each entry takes over at its activation epoch...
        assert_eq!(verifier.max_blobs_at(2).get(), 15);
        assert_eq!(verifier.max_blobs_at(3).get(), 15);
        // ...and the newest activated entry wins from then on.
        assert_eq!(verifier.max_blobs_at(4).get(), 21);
        assert_eq!(verifier.max_blobs_at(100).get(), 21);
    }

    #[test]
    fn shape_check_uses_the_limit_at_the_sidecars_epoch() {
        let verifier = KzgAdapter::new(
            [BlobParameters {
                epoch: 1,
                max_blobs_per_block: 15,
            }],
            NonZeroUsize::new(MAX_BLOBS).expect("nonzero"),
        );

        // 10 blobs in epoch 0: over the Electra fallback, rejected.
        let mut early = sidecar(3, MAX_BLOBS + 1);
        early.signed_block_header.message.slot = 0;
        assert!(matches!(
            verifier.check_shape(&early),
            Err(ValidationError::TooManyCommitments {
                maximum: MAX_BLOBS,
                ..
            })
        ));

        // The same sidecar one epoch later: within the raised limit, accepted.
        let mut later = early;
        later.signed_block_header.message.slot = SLOTS_PER_EPOCH;
        assert!(verifier.check_shape(&later).is_ok());
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

    /// A verified column as the store would hand back — the reconstructor's
    /// input type.
    fn verified_of(sidecar: &DataColumnSidecar) -> VerifiedColumn {
        let candidate = candidate_of(sidecar);
        VerifiedColumn::new_unchecked(candidate.id, candidate.context, candidate.payload)
    }

    #[test]
    fn reconstructs_the_missing_half_byte_identically() {
        let sidecars = real_sidecars();
        // Hold every even column — exactly half, interleaved with the gaps.
        let held: Vec<_> = sidecars
            .iter()
            .filter(|sidecar| sidecar.index % 2 == 0)
            .map(verified_of)
            .collect();

        let recovered = verifier().reconstruct(held).expect("recovery succeeds");

        // Every odd column comes back, byte-identical to the original: the
        // recovered cells and recomputed proofs match, and so does the copied
        // block-level metadata.
        assert_eq!(recovered.len(), 64);
        for (position, candidate) in recovered.iter().enumerate() {
            let expected_index = position as u64 * 2 + 1;
            assert_eq!(candidate.id.index(), expected_index);
            assert_eq!(
                candidate.payload,
                sidecars[expected_index as usize].as_ssz_bytes(),
            );
        }

        // Belt and braces: the recovered candidates pass the real verify
        // gate, exactly as they will on the node's re-admission path.
        let block = CandidateBlock::new(
            recovered[0].id.block_root(),
            recovered[0].context,
            recovered
                .into_iter()
                .map(|candidate| (candidate.id.index(), candidate.payload))
                .collect(),
        )
        .expect("a valid batch");
        for (index, verdict) in verifier().verify_block(block) {
            assert!(verdict.is_ok(), "recovered column {index} must re-verify");
        }
    }

    #[test]
    fn reconstruct_refuses_less_than_half() {
        let sidecars = real_sidecars();
        // 63 columns: one short of recoverable.
        let held: Vec<_> = sidecars.iter().take(63).map(verified_of).collect();
        assert!(matches!(
            verifier().reconstruct(held),
            Err(ValidationError::ReconstructionFailure(_))
        ));
    }

    /// `ream-da` and beacon each define `NUMBER_OF_COLUMNS` and neither may
    /// depend on the other; this adapter sees both, so it pins them equal.
    #[test]
    fn das_core_column_count_matches_beacon() {
        assert_eq!(
            ream_data_availability::id::NUMBER_OF_COLUMNS,
            ream_consensus_beacon::data_column_sidecar::NUMBER_OF_COLUMNS,
        );
    }
}
