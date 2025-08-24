#[cfg(test)]
mod hashsig_tests {
    use rand::rng;

    use crate::{
        hashsig::keystore,
        traits::{PQSignable, PQVerifiable},
    };

    #[test]
    fn test_sign_and_verify() {
        let mut rng = rng();
        let activation_epoch = 0;
        let num_active_epochs = 10; // Test for 10 epochs for quick key generation

        let (public_key, private_key) =
            keystore::generate(&mut rng, activation_epoch, num_active_epochs);

        let epoch = 5;

        // Create a test message (32 bytes as required by hashsig)
        let message = vec![0u8; 32];

        // Sign the message
        let result = private_key.sign(&message, epoch);

        assert!(result.is_ok(), "Signing should succeed");
        let signature = result.unwrap();

        // Verify the signature
        let verify_result = signature.verify(&message, &public_key, epoch);

        assert!(verify_result.is_ok(), "Verification should succeed");
        assert!(verify_result.unwrap(), "Signature should be valid");
    }
}
