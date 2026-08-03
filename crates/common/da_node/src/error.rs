use thiserror::Error;

/// Why a candidate could not be handed to the verification queue. Both
/// variants concern queue admission, not the candidate's validity.
#[derive(Debug, Error)]
pub enum IngestionError {
    /// The bounded verification queue is full; retryable.
    #[error("verification queue is full; candidate was not accepted")]
    Overloaded,

    /// The verification service has stopped; terminal.
    #[error("verification service has stopped; candidate was not accepted")]
    Closed,
}
