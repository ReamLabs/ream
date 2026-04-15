pub mod errors;
pub mod private_key;
pub mod public_key;
pub mod signature;

#[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
pub type LeanSigScheme = leansig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_32::hashing_optimized::SIGTopLevelTargetSumLifetime32Dim64Base8;

#[cfg(feature = "devnet4")]
pub type LeanSigScheme = leansig::signature::generalized_xmss::instantiations_aborting::lifetime_2_to_the_32::SchemeAbortingTargetSumLifetime32Dim46Base8;
