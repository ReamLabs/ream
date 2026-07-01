use thiserror::Error;

/// Why a candidate could not be handed to the verification queue.
///
/// Both variants concern queue *admission*, not the candidate's validity: a
/// candidate rejected here was never even verified. It is up to the producer to
/// decide how to react — retry, shed load, or surface the error to its caller.
#[derive(Debug, Error)]
pub enum IngestionError {
    /// The bounded verification queue is full. Returned by the non-blocking
    /// `try_submit` so the caller can shed load (e.g. answer an RPC with 503)
    /// instead of buffering candidates without bound.
    #[error("verification queue is full; candidate was not accepted")]
    Overloaded,

    /// The verification service has stopped and dropped its receiver, so no
    /// candidate can be accepted again. Terminal.
    #[error("verification service has stopped; candidate was not accepted")]
    Closed,
}
