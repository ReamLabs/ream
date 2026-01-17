use std::io::{self};

use ssz_types::{VariableList, typenum::U256};

#[derive(thiserror::Error, Debug)]
pub enum ReqRespError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Invalid data {0}")]
    InvalidData(String),

    #[error("Incomplete stream")]
    IncompleteStream,

    #[error("Stream timed out")]
    StreamTimedOut,

    #[error("Tokio timed out {0}")]
    TokioTimedOut(#[from] tokio::time::error::Elapsed),

    #[error("Disconnected")]
    Disconnected,

    #[error("Raw error message {0}")]
    RawError(String),
}

impl From<ssz::DecodeError> for ReqRespError {
    fn from(err: ssz::DecodeError) -> Self {
        ReqRespError::InvalidData(format!("Failed to decode ssz: {err:?}"))
    }
}

impl From<VariableList<u8, U256>> for ReqRespError {
    fn from(err: VariableList<u8, U256>) -> Self {
        let err = String::from_utf8(Vec::from(err)).unwrap_or("Invalid UTF-8".to_string());
        ReqRespError::InvalidData(format!("ReqResp error message from peer: {err:?}"))
    }
}
