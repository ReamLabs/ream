use thiserror::Error;

#[cfg(feature = "supranational")]
use crate::supranational::errors::BlstError;

#[derive(Error, PartialEq, Debug)]
pub enum BLSError {
    #[cfg(feature = "supranational")]
    #[error("blst error: {0}")]
    BlstError(#[from] BlstError),
    #[error("invalid byte length")]
    InvalidByteLength,
    #[error("invalid private key")]
    InvalidPrivateKey,
    #[error("invalid public key")]
    InvalidPublicKey,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("invalid hex string")]
    InvalidHexString,
}
