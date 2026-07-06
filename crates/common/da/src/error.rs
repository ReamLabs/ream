use std::io;

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    #[error("column index {column_index} is out of range 0..{number_of_columns}")]
    InvalidColumnIndex {
        column_index: u64,
        number_of_columns: u64,
    },

    #[error("malformed column payload: {0}")]
    MalformedPayload(String),

    #[error("column id mismatch: expected {expected}, got {actual}")]
    IdMismatch { expected: String, actual: String },

    #[error("slot mismatch: expected {expected}, got {actual}")]
    SlotMismatch { expected: u64, actual: u64 },

    #[error("column sidecar carries no commitments")]
    EmptyCommitments,

    #[error("too many commitments: {count} exceeds the per-block limit of {maximum}")]
    TooManyCommitments { count: usize, maximum: usize },

    #[error(
        "column sidecar length mismatch: {cells} cells, {commitments} commitments, {proofs} proofs"
    )]
    LengthMismatch {
        cells: usize,
        commitments: usize,
        proofs: usize,
    },

    #[error("commitments inclusion proof is invalid")]
    InvalidInclusionProof,

    #[error("column proof verification failed")]
    InvalidProof,

    #[error("verifier error: {0}")]
    VerifierFailure(String),
}

#[derive(Debug, Error)]
pub enum DaStoreError {
    /// Underlying storage failure: filesystem I/O, a missing backing file, or
    /// corruption. Not a normal "not found" answer — that is `Ok(None)`.
    #[error("storage I/O failure: {0}")]
    Io(#[from] io::Error),
}
