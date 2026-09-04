use crate::{
    column::{CandidateColumn, VerifiedColumn},
    error::ValidationError,
};

/// Rebuilds a block's missing columns from the ones already held.
pub trait ColumnReconstructor: Send + Sync {
    /// Recover every column of the block that is absent from `held`, returned
    /// as *candidates*.
    fn reconstruct(
        &self,
        held: Vec<VerifiedColumn>,
    ) -> Result<Vec<CandidateColumn>, ValidationError>;
}
