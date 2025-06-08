use std::time::Instant;

use hashsig::signature::{
    SignatureScheme,
    generalized_xmss::instantiations_poseidon::lifetime_2_to_the_20::winternitz::SIGWinternitzLifetime20W4,
};
use hashsig::signature::SignatureScheme;
use rand::{Rng, thread_rng};
use tracing::info;

pub fn generate_keys() {
    info!("Generating beam chain validator keys.....");
    let mut rng = thread_rng();
    // measure_time::<SIGWinternitzLifetime20W4, _>("Poseidon - L 20 - Winternitz - w 4", &mut rng);
    info!("Key generation complete");
}

fn measure_time<T: SignatureScheme, R: Rng>(description: &str, rng: &mut R) {
    let start = Instant::now();
    // let (_pk, _sk) = T::gen(rng);
    let duration = start.elapsed();
    info!("{} - Gen: {:?}", description, duration);
}
