pub mod errors;
pub mod private_key;
pub mod public_key;
pub mod scheme;
pub mod signature;

#[cfg(feature = "signature-scheme-prod")]
pub type HashSigScheme = hashsig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_32::hashing_optimized::SIGTopLevelTargetSumLifetime32Dim64Base8;

#[cfg(feature = "signature-scheme-test")]
pub type HashSigScheme = crate::hashsig::scheme::SIGTopLevelTargetSumLifetime8Dim16Base4;
