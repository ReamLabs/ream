use thiserror::Error;

#[derive(Debug, Error)]
pub enum LeanMultisigError {
    #[error("Key generation failed: {0}")]
    KeyGenerationFailed(#[from] anyhow::Error),

    #[error("Signing failed: {0}")]
    SigningFailed(anyhow::Error),

    #[error("Verification failed: {0}")]
    VerificationFailed(anyhow::Error),

    #[error("Serialization error: {0}")]
    SerializationError(anyhow::Error),

    #[error("Deserialization error: {0}")]
    DeserializationError(anyhow::Error),

    #[error("Invalid public key size")]
    InvalidPublicKeySize,
}
