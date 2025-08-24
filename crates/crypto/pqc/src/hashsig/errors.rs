#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("Signing failed: {0:?}")]
    SigningFailed(hashsig::signature::SigningError),
}
