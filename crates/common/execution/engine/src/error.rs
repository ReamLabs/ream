use alloy_primitives::B256;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum EngineError {
    #[error("Block payload is invalid: latest_valid_hash={latest_valid_hash:?}")]
    InvalidPayload { latest_valid_hash: Option<B256> },
    #[error("Block payload block hash is invalid")]
    InvalidBlockHash,
}
