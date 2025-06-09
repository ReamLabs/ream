use hashsig::signature::{
    SignatureScheme,
    generalized_xmss::instantiations_poseidon::lifetime_2_to_the_20::winternitz::SIGWinternitzLifetime20W4,
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tracing::info;

pub fn generate_keys(seed_phrase: &str) {
    info!("Generating beam chain validator keys.....");

    // Convert seed phrase to bytes for RNG initialization
    let seed_bytes = seed_phrase.as_bytes();
    let mut rng = ChaCha20Rng::from_seed(seed_bytes.try_into().unwrap_or([0; 32]));

    // measure_time::<SIGWinternitzLifetime20W4, _>("Poseidon - L 20 - Winternitz - w 4", &mut rng);
    let (_pk, _sk) = SIGWinternitzLifetime20W4::r#gen(&mut rng);
    info!("Generated XMSS key pair with lifetime 2^20");
    info!("Key generation complete");
}
