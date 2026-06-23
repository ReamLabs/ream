#[derive(Debug, thiserror::Error)]
pub enum DataColumnSidecarError {
    #[error(
        "cells_and_kzg_proofs length {actual} does not match kzg_commitments length {expected}"
    )]
    CommitmentCountMismatch { actual: usize, expected: usize },
    #[error("column index {0} out of bounds for blob cells/proofs (expected {1} entries)")]
    ColumnIndexOutOfBounds(usize, usize),
    #[error("failed to create VariableList for column {column_index}: {err}")]
    DecodingError { column_index: u64, err: String },
    #[error("failed to compute blob kzg commitments inclusion proof: {0}")]
    InclusionProofError(String),
}
