use crate::{
    column::{CandidateColumn, VerifiedColumn},
    error::ValidationError,
};

pub trait DaVerifier: Send + Sync {
    fn verify(&self, candidate: CandidateColumn) -> Result<VerifiedColumn, ValidationError>;
}
