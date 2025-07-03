use hashsig::signature::{
    SignatureScheme,
    generalized_xmss::instantiations_poseidon::lifetime_2_to_the_20::winternitz::SIGWinternitzLifetime20W4,
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use sha2::{Digest, Sha256};
use tracing::info;

pub fn generate_keys(seed_phrase: &str) {
    info!("Generating beam chain validator keys.....");

    // Hash the seed phrase to get a 32-byte seed
    let mut hasher = Sha256::new();
    hasher.update(seed_phrase.as_bytes());
    let seed = hasher.finalize().into();
    info!("Seed: {seed:?}");

    let mut rng = <ChaCha20Rng as SeedableRng>::from_seed(seed);

    // measure_time::<SIGWinternitzLifetime20W4, _>("Poseidon - L 20 - Winternitz - w 4", &mut rng);
    let (_public_key, _secret_key) = SIGWinternitzLifetime20W4::r#gen(&mut rng);
    info!("Generated XMSS key pair with lifetime 2^20");
    info!("Key generation complete");
}
