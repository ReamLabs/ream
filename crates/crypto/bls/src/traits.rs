use crate::{errors::BLSError, AggregatePubKey, PubKey};

/// Trait for aggregating BLS public keys.
///
/// This trait provides functionality to combine multiple BLS public keys into a single
/// aggregate public key. This is useful for signature verification of messages signed
/// by multiple parties.
pub trait Aggregatable {
    type Error;

    /// Aggregates multiple BLS public keys into a single aggregate public key.
    ///
    /// # Arguments
    /// * `pubkeys` - Slice of public key references to aggregate
    ///
    /// # Returns
    /// * `Result<AggregatePubKey, Self::Error>` - The aggregated public key or an error
    fn aggregate(pubkeys: &[&PubKey]) -> Result<AggregatePubKey, Self::Error>;
}

/// Marker trait for zkcrypto/bls12_381 BLS public key aggregation implementation
pub trait ZkcryptoAggregatable: Aggregatable<Error = BLSError> {}

/// Marker trait for supranational/blst BLS public key aggregation implementation
pub trait SupranationalAggregatable: Aggregatable<Error = anyhow::Error> {}

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
    /// * `pubkey` - The public key to verify against
    /// * `message` - The message that was signed
    ///
    /// # Returns
    /// * `Result<bool, BLSError>` - Ok(true) if the signature is valid, Ok(false) if verification
    ///   fails, or Err if there are issues with signature or public key bytes
    fn verify(&self, pubkey: &PubKey, message: &[u8]) -> Result<bool, Self::Error>;

    /// Verifies the signature against a message using an aggregate of multiple public keys
    ///
    /// # Arguments
    /// * `pubkeys` - Collection of public key references to verify against
    /// * `message` - Message that was signed
    ///
    /// # Returns
    /// * `Result<bool, BLSError>` - Ok(true) if the signature is valid for the aggregate
    ///   verification, Ok(false) if verification fails, or Err if there are issues with signature
    ///   or public key bytes
    fn fast_aggregate_verify<'a, P>(&self, pubkeys: P, message: &[u8]) -> Result<bool, Self::Error>
    where
        P: AsRef<[&'a PubKey]>;
}

/// Marker trait for zkcrypto/bls12_381 BLS signature verification implementation
pub trait ZkcryptoVerifiable: Verifiable<Error = BLSError> {}

/// Marker trait for supranational/blst BLS signature verification implementation
pub trait SupranationalVerifiable: Verifiable<Error = BLSError> {}
