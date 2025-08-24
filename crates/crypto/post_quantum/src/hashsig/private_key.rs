use hashsig::{MESSAGE_LENGTH, signature::SignatureScheme};
use rand::Rng;

use crate::hashsig::{
    HashSigScheme, errors::SigningError, public_key::PublicKey, signature::Signature,
};

pub type HashSigPrivateKey = <HashSigScheme as SignatureScheme>::SecretKey;

pub struct PrivateKey {
    inner: HashSigPrivateKey,
}

impl PrivateKey {
    pub fn new(inner: HashSigPrivateKey) -> Self {
        Self { inner }
    }

    pub fn generate<R: Rng>(
        rng: &mut R,
        activation_epoch: usize,
        num_active_epochs: usize,
    ) -> (PublicKey, Self) {
        let (public_key, private_key) =
            <HashSigScheme as SignatureScheme>::key_gen(rng, activation_epoch, num_active_epochs);

        (PublicKey::new(public_key), Self::new(private_key))
    }

    pub fn sign<R: Rng>(
        &self,
        rng: &mut R,
        message: &[u8; MESSAGE_LENGTH],
        epoch: u32,
    ) -> anyhow::Result<Signature, SigningError> {
        Ok(Signature::new(
            <HashSigScheme as SignatureScheme>::sign(rng, &self.inner, epoch, message)
                .map_err(SigningError::SigningFailed)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use rand::rng;

    use crate::hashsig::private_key::PrivateKey;

    #[test]
    fn test_sign_and_verify() {
        let mut rng = rng();
        let activation_epoch = 0;
        let num_active_epochs = 10; // Test for 10 epochs for quick key generation

        let (public_key, private_key) =
            PrivateKey::generate(&mut rng, activation_epoch, num_active_epochs);

        let epoch = 5;

        // Create a test message (32 bytes as required by hashsig)
        let message = [0u8; 32];

        // Sign the message
        let result = private_key.sign(&mut rng, &message, epoch);

        assert!(result.is_ok(), "Signing should succeed");
        let signature = result.unwrap();

        // Verify the signature
        let verify_result = signature.verify(&message, &public_key, epoch);

        assert!(verify_result.is_ok(), "Verification should succeed");
        assert!(verify_result.unwrap(), "Signature should be valid");
    }
}
