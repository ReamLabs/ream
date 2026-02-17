use leansig::{signature::SignatureScheme, MESSAGE_LENGTH};
use serde::{Deserialize, Serialize};
use ssz::{Decode, DecodeError, Encode};

use crate::leansig::{public_key::PublicKey, LeanSigScheme};

/// The inner leansig signature type with built-in SSZ support.
pub type LeanSigSignature = <LeanSigScheme as SignatureScheme>::Signature;

/// Wrapper around leansig's signature type.
/// Uses leansig's built-in SSZ encoding for interoperability with other clients.
#[derive(Clone, Serialize, Deserialize)]
pub struct Signature {
    pub inner: LeanSigSignature,
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signature")
            .field("inner", &"<LeanSigSignature>")
            .finish()
    }
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        // Compare by SSZ encoding since LeanSigSignature doesn't implement PartialEq
        self.inner.as_ssz_bytes() == other.inner.as_ssz_bytes()
    }
}

impl Eq for Signature {}

impl Encode for Signature {
    fn is_ssz_fixed_len() -> bool {
        <LeanSigSignature as Encode>::is_ssz_fixed_len()
    }

    fn ssz_bytes_len(&self) -> usize {
        self.inner.ssz_bytes_len()
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        self.inner.ssz_append(buf)
    }
}

impl Decode for Signature {
    fn is_ssz_fixed_len() -> bool {
        <LeanSigSignature as Decode>::is_ssz_fixed_len()
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            inner: LeanSigSignature::from_ssz_bytes(bytes)?,
        })
    }
}

impl Signature {
    pub fn new(inner: LeanSigSignature) -> Self {
        Self { inner }
    }

    /// Create a blank/placeholder signature.
    ///
    /// This decodes from minimal valid SSZ bytes, avoiding expensive key generation.
    /// Only use in contexts where the signature won't be validated.
    pub fn blank() -> Self {
        // 40 bytes: offset_path(4) + rho(28 zeros) + offset_hashes(4) + path(4)
        const BYTES: [u8; 40] = [
            36, 0, 0, 0, // offset_path = 36
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // rho (28 zeros)
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, //
            40, 0, 0, 0, // offset_hashes = 40
            4, 0, 0, 0, // path: empty HashTreeOpening
        ];
        Self::from_ssz_bytes(&BYTES).expect("blank signature bytes are valid")
    }

    /// Create a mock signature for testing purposes.
    ///
    /// Note: This generates a real signature which is expensive. Prefer `blank()` when
    /// you just need a placeholder signature.
    pub fn mock() -> Self {
        use rand::rng;

        use crate::leansig::private_key::PrivateKey;

        let mut rng = rng();
        let (_, private_key) = PrivateKey::generate_key_pair(&mut rng, 0, 10);
        let message = [0u8; 32];
        private_key
            .sign(&message, 0)
            .expect("Mock signature generation failed")
    }

    pub fn from_lean_sig(signature: LeanSigSignature) -> Self {
        Self { inner: signature }
    }

    pub fn as_lean_sig(&self) -> &LeanSigSignature {
        &self.inner
    }

    pub fn verify(
        &self,
        public_key: &PublicKey,
        epoch: u32,
        message: &[u8; MESSAGE_LENGTH],
    ) -> anyhow::Result<bool> {
        Ok(<LeanSigScheme as SignatureScheme>::verify(
            &public_key.as_lean_sig()?,
            epoch,
            message,
            &self.inner,
        ))
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::FixedBytes;
    use leansig::serialization::Serializable;
    use rand::rng;
    use ssz::{Decode, Encode};

    use crate::leansig::{private_key::PrivateKey, signature::Signature};

    const LEGACY_SIGNATURE_SIZE: usize = 3112;

    #[derive(ssz_derive::Encode)]
    struct LegacySignature {
        inner: FixedBytes<LEGACY_SIGNATURE_SIZE>,
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut rng = rng();
        let activation_epoch = 0;
        let num_active_epochs = 10; // Test for 10 epochs for quick key generation

        let (_, private_key) =
            PrivateKey::generate_key_pair(&mut rng, activation_epoch, num_active_epochs);

        let epoch = 5;

        // Create a test message (32 bytes as required by leansig)
        let message = [0u8; 32];

        // Sign the message
        let result = private_key.sign(&message, epoch);

        assert!(result.is_ok(), "Signing should succeed");
        let signature = result.unwrap();

        // SSZ roundtrip test
        let ssz_bytes = signature.as_ssz_bytes();
        let signature_decoded = Signature::from_ssz_bytes(&ssz_bytes).unwrap();

        // verify roundtrip
        assert_eq!(signature, signature_decoded);
    }

    #[test]
    fn test_ssz_bytes_match_legacy_signature_wrapper() {
        let mut rng = rng();
        let activation_epoch = 0;
        let num_active_epochs = 10;

        let (_, private_key) =
            PrivateKey::generate_key_pair(&mut rng, activation_epoch, num_active_epochs);

        let epoch = 5;
        let message = [0u8; 32];
        let signature = private_key.sign(&message, epoch).unwrap();

        let legacy_signature = LegacySignature {
            inner: FixedBytes::try_from(signature.as_lean_sig().to_bytes().as_slice())
                .expect("legacy signature bytes should match fixed size"),
        };

        assert_eq!(legacy_signature.as_ssz_bytes(), signature.as_ssz_bytes());
    }
}
