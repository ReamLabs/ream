pub mod errors;
pub mod private_key;
pub mod public_key;
pub mod signature;

use hashsig::signature::generalized_xmss::instantiations_poseidon::lifetime_2_to_the_18::winternitz::SIGWinternitzLifetime18W4;

pub type HashSigScheme = SIGWinternitzLifetime18W4;
