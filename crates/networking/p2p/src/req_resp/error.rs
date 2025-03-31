use std::io;

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

    #[error("Stream timed out {0}")]
    StreamTimedOut(#[from] tokio::time::error::Elapsed),
}

impl From<ssz::DecodeError> for ReqRespError {
    fn from(err: ssz::DecodeError) -> Self {
        ReqRespError::InvalidData(format!("Failed to decode ssz: {err:?}"))
    }
}
