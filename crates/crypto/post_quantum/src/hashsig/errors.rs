#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("Signing failed: {0:?}")]
    SigningFailed(hashsig::signature::SigningError),

    #[error("Signature serialization failed")]
    SerializationFailed,

    #[error("Invalid signature length")]
    InvalidSignatureLength,

    #[error("Signature decode failed: {0:?}")]
    SignatureDecodeFailed(bincode::error::DecodeError),
}
