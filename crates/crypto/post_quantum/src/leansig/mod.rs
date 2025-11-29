pub mod errors;
pub mod private_key;
pub mod public_key;
pub mod signature;

#[cfg(all(not(test), feature = "signature-scheme-prod"))]
pub type LeanSigScheme = leansig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_32::hashing_optimized::SIGTopLevelTargetSumLifetime32Dim64Base8;
#[cfg(all(not(test), feature = "signature-scheme-prod"))]
const SIGNATURE_SIZE: usize = 3112;

#[cfg(all(not(test), feature = "signature-scheme-test"))]
pub type LeanSigScheme = leansig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_8::SIGTopLevelTargetSumLifetime8Dim64Base8;
#[cfg(all(not(test), feature = "signature-scheme-test"))]
const SIGNATURE_SIZE: usize = 2344;

#[cfg(test)]
pub type LeanSigScheme = leansig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_8::SIGTopLevelTargetSumLifetime8Dim64Base8;
#[cfg(test)]
const SIGNATURE_SIZE: usize = 2344;
