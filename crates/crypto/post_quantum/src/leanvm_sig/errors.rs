#[derive(Debug, thiserror::Error)]
pub enum LeanSigError {
    #[error("Signing failed: {0}")]
    SigningFailed(lean_multisig_leanvm::XmssSignatureError),

    #[error("Key generation failed: {0}")]
    KeyGenFailed(lean_multisig_leanvm::XmssKeyGenError),

    #[error("TryFromSliceError error: {0:?}")]
    TryFromSliceError(core::array::TryFromSliceError),

    #[error("Invalid signature length: {0}")]
    InvalidSignatureLength(usize),

    #[error("Deserialization error: {0}")]
    DeserializationError(anyhow::Error),
}

impl From<core::array::TryFromSliceError> for LeanSigError {
    fn from(err: core::array::TryFromSliceError) -> Self {
        LeanSigError::TryFromSliceError(err)
    }
}
