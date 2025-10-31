pub mod errors;
pub mod private_key;
pub mod public_key;
pub mod signature;

#[cfg(test)]
pub type HashSigScheme = hashsig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_8::SIGTopLevelTargetSumLifetime8Dim64Base8;

#[cfg(all(not(test), feature = "signature-scheme-prod"))]
pub type HashSigScheme = hashsig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_32::hashing_optimized::SIGTopLevelTargetSumLifetime32Dim64Base8;

#[cfg(all(not(test), feature = "signature-scheme-test"))]
pub type HashSigScheme = hashsig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_8::SIGTopLevelTargetSumLifetime8Dim64Base8;
