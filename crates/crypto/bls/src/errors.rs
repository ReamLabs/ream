use thiserror::Error;

#[cfg(feature = "supranational")]
use crate::supranational::errors::BlstError;

#[derive(Error, Debug)]
pub enum BLSError {
    #[cfg(feature = "supranational")]
    #[error("blst error: {0}")]
    BlstError(#[from] BlstError),
    #[error("invalid byte length {0}")]
    InvalidByteLength(anyhow::Error),
    #[error("invalid private key {0}")]
    InvalidPrivateKey(anyhow::Error),
    #[error("invalid public key {0}")]
    InvalidPublicKey(anyhow::Error),
    #[error("invalid signature {0}")]
    InvalidSignature(anyhow::Error),
    #[error("invalid hex string")]
    InvalidHexString(const_hex::FromHexError),
}
