use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use ream_pqc::hashsig::keystore;
use sha2::{Digest, Sha256};
use tracing::info;

pub fn generate_keys(
    seed_phrase: &str,
    activation_epoch: usize,
    num_active_epochs: usize,
) -> (ream_pqc::hashsig::PublicKey, ream_pqc::hashsig::PrivateKey) {
    info!("Generating beam chain validator keys.....");

    // Hash the seed phrase to get a 32-byte seed
    let mut hasher = Sha256::new();
    hasher.update(seed_phrase.as_bytes());
    let seed = hasher.finalize().into();
    info!("Seed: {seed:?}");

    let mut rng = <ChaCha20Rng as SeedableRng>::from_seed(seed);

    info!(
        "Generating hash-based signature key pair with activation_epoch={}, num_active_epochs={}",
        activation_epoch, num_active_epochs
    );
    let (public_key, private_key) =
        keystore::generate(&mut rng, activation_epoch, num_active_epochs);
    info!("Key generation complete");

    (public_key, private_key)
}
