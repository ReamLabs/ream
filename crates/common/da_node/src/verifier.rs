use ream_da::{
    column::{CandidateColumn, VerifiedColumn},
    error::ValidationError,
    verifier::DaVerifier,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct KzgVerifier {}

impl DaVerifier for KzgVerifier {
    fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError> {
        Ok(VerifiedColumn::new_unchecked(
            candidate.id,
            candidate.context,
            candidate.payload,
        ))
    }
}
