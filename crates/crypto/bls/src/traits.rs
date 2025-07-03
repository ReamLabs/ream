use crate::{BLSSignature, PublicKey, errors::BLSError};

/// Trait for aggregating BLS public keys.
///
/// This trait provides functionality to combine multiple BLS public keys into a single
/// aggregate public key. This is useful for signature verification of messages signed
/// by multiple parties.
pub trait Aggregatable<T> {
    type Error;

    /// Aggregates multiple BLS items into a single aggregate item.
    ///
    /// # Arguments
    /// * `items` - Slice of references to the items to aggregate
    ///
    /// # Returns
    /// * `Result<Self::Output, Self::Error>` - The aggregated item or an error
    fn aggregate(items: &[&T]) -> Result<T, Self::Error>;
}

/// Marker trait for zkcrypto/bls12_381 BLS aggregation implementation
pub trait ZkcryptoAggregatable<T>: Aggregatable<T, Error = BLSError> {}

/// Marker trait for supranational/blst BLS aggregation implementation
pub trait SupranationalAggregatable<T>: Aggregatable<T, Error = anyhow::Error> {}

/// Trait for BLS message signing.
///
/// This trait provides functionality to sign messages using a BLS private key.
pub trait Signable {
    type Error;

    /// Signs a message using the private key.
    ///
    /// # Arguments
    /// * `message` - The message bytes to sign
    ///
    /// # Returns
    /// * `Result<BLSSignature, Self::Error>` - The BLS signature or an error
    fn sign(&self, message: &[u8]) -> Result<BLSSignature, Self::Error>;
}

/// Marker trait for zkcrypto/bls12_381 BLS signing implementation
pub trait ZkcryptoSignable: Signable<Error = BLSError> {}

/// Marker trait for supranational/blst BLS signing implementation
pub trait SupranationalSignable: Signable<Error = anyhow::Error> {}

/// Trait for verifying BLS signatures.
///
/// This trait provides functionality to verify both individual and aggregate BLS signatures
/// against messages. It supports both single-key verification and fast aggregate verification
/// against multiple public keys.
pub trait Verifiable {
    type Error;

    /// Verifies a BLS signature against a public key and message.
    ///
    /// # Arguments
    /// * `public_key` - The public key to verify against
    /// * `message` - The message that was signed
    ///
    /// # Returns
    /// * `Result<bool, BLSError>` - Ok(true) if the signature is valid, Ok(false) if verification
    ///   fails, or Err if there are issues with signature or public key bytes
    fn verify(&self, public_key: &PublicKey, message: &[u8]) -> Result<bool, Self::Error>;

    /// Verifies the signature against a message using an aggregate of multiple public keys
    ///
    /// # Arguments
    /// * `public_keys` - Collection of public key references to verify against
    /// * `message` - Message that was signed
    ///
    /// # Returns
    /// * `Result<bool, BLSError>` - Ok(true) if the signature is valid for the aggregate
    ///   verification, Ok(false) if verification fails, or Err if there are issues with signature
    ///   or public key bytes
    fn fast_aggregate_verify<'a, P>(
        &self,
        public_keys: P,
        message: &[u8],
    ) -> Result<bool, Self::Error>
    where
        P: AsRef<[&'a PublicKey]>;
}

/// Marker trait for zkcrypto/bls12_381 BLS signature verification implementation
pub trait ZkcryptoVerifiable: Verifiable<Error = BLSError> {}

/// Marker trait for supranational/blst BLS signature verification implementation
pub trait SupranationalVerifiable: Verifiable<Error = BLSError> {}
