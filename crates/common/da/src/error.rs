use std::io;

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    #[error("column index {column_index} is outside 0..{number_of_columns}")]
    InvalidColumnIndex {
        column_index: u64,
        number_of_columns: u64,
    },
}

#[derive(Debug, Error)]
pub enum DaStoreError {
    /// Underlying storage failure: filesystem I/O, a missing backing file, or
    /// corruption. Not a normal "not found" answer — that is `Ok(None)`.
    #[error("storage I/O failure: {0}")]
    Io(#[from] io::Error),
}
