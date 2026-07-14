use crate::{
    column::{CandidateColumn, VerifiedColumn},
    error::ValidationError,
};

/// Rebuilds a block's missing columns from the ones already held.
pub trait ColumnReconstructor: Send + Sync {
    /// Recover every column of the block that is absent from `held`, returned
    /// as *candidates*.
    ///
    /// The output is deliberately not [`VerifiedColumn`]: the recovery math is
    /// sound, but the surrounding assembly (row transposition, metadata
    /// copying, re-encoding) is ordinary code, so recovered columns re-enter
    /// through the same verify-then-store gate as network columns. Only the
    /// verifier mints [`VerifiedColumn`]; the store never holds anything
    /// self-attested.
    fn reconstruct(
        &self,
        held: Vec<VerifiedColumn>,
    ) -> Result<Vec<CandidateColumn>, ValidationError>;
}
