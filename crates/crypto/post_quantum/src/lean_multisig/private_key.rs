use anyhow::anyhow;
use lean_multisig::{XmssSecretKey, xmss_key_gen};
use rand::Rng;

use crate::lean_multisig::{
    errors::{LeanMultisigError, LeanMultisigError::KeyGenerationFailed},
    public_key::PublicKey,
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

        Ok((PublicKey::from_xmss(public_key)?, Self::new(secret_key)))
    }

    /// Returns a reference to the inner XmssSecretKey for signing operations.
    pub fn inner(&self) -> &XmssSecretKey {
        &self.inner
    }

    /// Returns the public key corresponding to this private key.
    pub fn public_key(&self) -> Result<PublicKey, LeanMultisigError> {
        PublicKey::from_xmss(self.inner.public_key())
    }
}

#[cfg(test)]
mod tests {
    use rand::rng;

    use crate::lean_multisig::private_key::PrivateKey;

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
}
