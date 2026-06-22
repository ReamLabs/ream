use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    #[error("column index {column_index} is outside 0..{number_of_columns}")]
    InvalidColumnIndex {
        column_index: u64,
        number_of_columns: u64,
    },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DaStoreError {}
