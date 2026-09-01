use crate::{
    column::{CandidateBlock, CandidateColumn, VerifiedColumn},
    error::ValidationError,
};

pub trait ColumnVerifier: Send + Sync {
    fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError>;
    fn verify_block(
        &self,
        block: CandidateBlock,
    ) -> Vec<(u64, Result<VerifiedColumn, ValidationError>)> {
        block
            .into_columns()
            .map(|candidate| (candidate.id.index(), self.verify(candidate)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;

    use super::ColumnVerifier;
    use crate::{
        column::{CandidateBlock, CandidateColumn, ColumnContext, VerifiedColumn},
        error::ValidationError,
    };

    /// A stub that accepts even column indices and rejects odd ones, so a
    /// batch exercises both verdict kinds in one pass.
    struct RejectOddIndices;

    impl ColumnVerifier for RejectOddIndices {
        fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError> {
            if candidate.id.index() % 2 == 1 {
                return Err(ValidationError::InvalidProof);
            }
            Ok(VerifiedColumn::new_unchecked(
                candidate.id,
                candidate.context,
                candidate.payload,
            ))
        }
    }

    #[test]
    fn default_verify_block_yields_one_tagged_verdict_per_column() {
        let block = CandidateBlock::new(
            B256::repeat_byte(1),
            ColumnContext { slot: 9 },
            (0..4).map(|index| (index, vec![])).collect(),
        )
        .expect("a valid batch");

        let verdicts = RejectOddIndices.verify_block(block);

        // One verdict per column, tagged, in submission order; a rejected
        // column does not disturb its neighbours.
        assert_eq!(verdicts.len(), 4);
        for (position, (index, verdict)) in verdicts.iter().enumerate() {
            assert_eq!(*index, position as u64);
            match index % 2 {
                0 => {
                    let verified = verdict.as_ref().expect("even indices are accepted");
                    assert_eq!(verified.id().index(), *index);
                    assert_eq!(verified.context().slot, 9);
                }
                _ => assert_eq!(verdict, &Err(ValidationError::InvalidProof)),
            }
        }
    }
}
