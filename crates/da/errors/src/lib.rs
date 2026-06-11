use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaError {
    /// The URL provided for the beacon API is not valid.
    #[error("invalid beacon URL: {0}")]
    InvalidBeaconUrl(String),

    /// The SSE event stream could not be established or was lost.
    #[error("beacon event stream error: {0}")]
    EventStreamFailed(String),

    /// A consensus event was received but could not be decoded.
    #[error("failed to decode consensus event: {0}")]
    EventDecodeFailed(String),

    /// A column sidecar could not be written to disk.
    #[error("failed to store column (block_root={block_root}, index={index}): {source}")]
    ColumnWriteFailed {
        block_root: String,
        index: u64,
        source: anyhow::Error,
    },

    /// A column sidecar could not be read from disk.
    #[error("failed to read column (block_root={block_root}, index={index}): {source}")]
    ColumnReadFailed {
        block_root: String,
        index: u64,
        source: anyhow::Error,
    },

    /// The slot index database returned an error.
    #[error("slot index error: {0}")]
    SlotIndexFailed(#[from] anyhow::Error),

    /// A gossip message could not be decoded.
    #[error("gossip decode error: {0}")]
    GossipDecodeFailed(String),

    /// KZG proof verification failed.
    #[error("KZG verification failed: {0}")]
    KzgVerificationFailed(String),

    /// Reconstruction of missing columns from partial data failed.
    #[error("column reconstruction failed: {0}")]
    ReconstructionFailed(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// Shorthand Result alias used across DA crates.
pub type DaResult<T> = std::result::Result<T, DaError>;
