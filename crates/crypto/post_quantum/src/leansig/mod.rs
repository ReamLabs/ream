pub mod errors;
pub mod private_key;
pub mod public_key;
pub mod signature;

pub type LeanSigScheme = leansig::signature::generalized_xmss::instantiations_aborting::lifetime_2_to_the_32::SchemeAbortingTargetSumLifetime32Dim46Base8;
