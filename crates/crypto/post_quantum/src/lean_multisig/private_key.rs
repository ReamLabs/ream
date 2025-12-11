use anyhow::anyhow;
use lean_multisig::{F, XmssSecretKey, xmss_key_gen};
use rand::Rng;

use crate::lean_multisig::{
    errors::LeanMultisigError::{self, KeyGenerationFailed},
    public_key::PublicKey,
    signature::Signature,
};

pub struct PrivateKey {
    pub inner: XmssSecretKey,
}

impl PrivateKey {
    pub fn new(inner: XmssSecretKey) -> Self {
        Self { inner }
    }

    /// Generates a new key pair with the given parameters.
    ///
    /// # Arguments
    /// * `rng` - Random number generator for generating the seed
    /// * `first_slot` - The first slot number for which this key is valid
    /// * `log_lifetime` - Log2 of the number of signatures this key can produce
    ///
    /// # Returns
    /// A tuple of (PublicKey, PrivateKey)
    pub fn generate_key_pair<R: Rng>(
        rng: &mut R,
        first_slot: u64,
        log_lifetime: usize,
    ) -> Result<(PublicKey, Self), LeanMultisigError> {
        // Generate a random seed
        let mut seed = [0u8; 32];
        rng.fill(&mut seed);

        let (secret_key, public_key) = xmss_key_gen(seed, first_slot, log_lifetime)
            .map_err(|err| KeyGenerationFailed(anyhow!("{err:?}")))?;

        Ok((PublicKey::from_xmss(&public_key)?, Self::new(secret_key)))
    }

    /// Returns a reference to the inner XmssSecretKey for signing operations.
    pub fn inner(&self) -> &XmssSecretKey {
        &self.inner
    }

    /// Returns the public key corresponding to this private key.
    pub fn public_key(&self) -> Result<PublicKey, LeanMultisigError> {
        PublicKey::from_xmss(&self.inner.public_key())
    }

    pub fn sign<R: Rng>(
        &self,
        rng: &mut R,
        message_hash: [F; 8],
        slot: u64,
    ) -> Result<Signature, LeanMultisigError> {
        let mut randomness_seed = [0u8; 32];
        rng.fill(&mut randomness_seed);

        let xmss_signature =
            lean_multisig::xmss_sign(randomness_seed, &self.inner, &message_hash, slot).map_err(
                |err| {
                    LeanMultisigError::SigningFailed(anyhow!(
                        "Failed to get XmssSignature: {err:?}"
                    ))
                },
            )?;

        Signature::from_xmss(&xmss_signature)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use lean_multisig::{F, PrimeCharacteristicRing, xmss_verify};
    use rand::rng;

    use crate::lean_multisig::{
        errors::LeanMultisigError, private_key::PrivateKey, public_key::PublicKey,
        signature::Signature,
    };

    #[test]
    fn test_generate_key_pair() {
        let mut rng = rng();
        let first_slot = 0;
        let log_lifetime = 10;

        let (public_key, private_key) =
            PrivateKey::generate_key_pair(&mut rng, first_slot, log_lifetime).unwrap();

        let derived_public_key = private_key.public_key().unwrap();
        assert_eq!(public_key, derived_public_key);
    }

    #[test]
    fn test_sign_and_verify() -> Result<(), LeanMultisigError> {
        let mut rng = rng();
        let first_slot = 100;
        let log_lifetime = 10;
        let slot = first_slot + 5;

        let (public_key, private_key) =
            PrivateKey::generate_key_pair(&mut rng, first_slot, log_lifetime).unwrap();

        let message_hash: [F; 8] = std::array::from_fn(|i| F::from_usize(i * 1000));
        let signature = private_key.sign(&mut rng, message_hash, slot)?;

        let xmss_public_key = public_key.as_xmss().map_err(|err| {
            LeanMultisigError::DeserializationError(anyhow!("Failed to get public key: {err:?}"))
        })?;

        assert_eq!(
            PublicKey::from_xmss(&xmss_public_key)?,
            public_key,
            "Public Key decoding failed."
        );
        let xmss_signature = signature.as_xmss().map_err(|err| {
            LeanMultisigError::DeserializationError(anyhow!("Failed to get signature: {err:?}"))
        })?;

        assert_eq!(
            Signature::from_xmss(&xmss_signature)?,
            signature,
            "Signature decoding failed."
        );

        xmss_verify(&xmss_public_key, &message_hash, &xmss_signature, slot).map_err(|err| {
            LeanMultisigError::VerificationFailed(anyhow!("Failed to verify: {err:?}"))
        })?;

        Ok(())
    }
}
