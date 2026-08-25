use crate::{
    column::{CandidateColumn, VerifiedColumn},
    error::ValidationError,
};

pub trait ColumnVerifier: Send + Sync {
    fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError>;
}
