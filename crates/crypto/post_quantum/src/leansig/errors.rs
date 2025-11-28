#[derive(Debug, thiserror::Error)]
pub enum LeanSigError {
    #[error("Signing failed: {0:?}")]
    SigningFailed(leansig::signature::SigningError),

    #[error("TryFromSliceError error: {0:?}")]
    TryFromSliceError(core::array::TryFromSliceError),

    #[error("Invalid signature length: {0}")]
    InvalidSignatureLength(usize),
}

impl From<core::array::TryFromSliceError> for LeanSigError {
    fn from(err: core::array::TryFromSliceError) -> Self {
        LeanSigError::TryFromSliceError(err)
    }
}
