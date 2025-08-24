use hashsig::MESSAGE_LENGTH;

#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("Message length must be exactly {MESSAGE_LENGTH} bytes, got {0}")]
    InvalidMessageLength(usize),
    #[error("Message conversion failed: {0}")]
    MessageConversionFailed(#[from] std::array::TryFromSliceError),
    #[error("Signing failed: {0:?}")]
    SigningFailed(hashsig::signature::SigningError),
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Message length must be exactly {MESSAGE_LENGTH} bytes, got {0}")]
    InvalidMessageLength(usize),
    #[error("Message conversion failed: {0}")]
    MessageConversionFailed(#[from] std::array::TryFromSliceError),
}
