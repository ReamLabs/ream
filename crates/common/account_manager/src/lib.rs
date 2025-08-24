use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use ream_post_quantum_crypto::hashsig::{private_key::PrivateKey, public_key::PublicKey};
use sha2::{Digest, Sha256};
use tracing::info;

pub fn generate_keys(
    seed_phrase: &str,
    activation_epoch: usize,
    num_active_epochs: usize,
) -> (PublicKey, PrivateKey) {
    info!("Generating lean consensus validator keys.....");

    // Hash the seed phrase to get a 32-byte seed
    let mut hasher = Sha256::new();
    hasher.update(seed_phrase.as_bytes());
    let seed = hasher.finalize().into();
    info!("Seed: {seed:?}");

    info!(
        "Generating hash-based signature key pair with activation_epoch={activation_epoch}, num_active_epochs={num_active_epochs}"
    );

    info!("Key generation complete");

    PrivateKey::generate(
        &mut <ChaCha20Rng as SeedableRng>::from_seed(seed),
        activation_epoch,
        num_active_epochs,
    )
}
