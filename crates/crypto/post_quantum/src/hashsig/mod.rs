pub mod errors;
pub mod private_key;
pub mod public_key;
pub mod signature;

use bincode::config::{Fixint, LittleEndian, NoLimit};

#[cfg(all(not(test), feature = "signature-scheme-prod"))]
pub type HashSigScheme = hashsig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_32::hashing_optimized::SIGTopLevelTargetSumLifetime32Dim64Base8;

#[cfg(all(not(test), feature = "signature-scheme-test"))]
pub type HashSigScheme = hashsig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_8::SIGTopLevelTargetSumLifetime8Dim64Base8;

#[cfg(test)]
pub type HashSigScheme = hashsig::signature::generalized_xmss::instantiations_poseidon_top_level::lifetime_2_to_the_8::SIGTopLevelTargetSumLifetime8Dim64Base8;

/// NOTE: `GeneralizedXMSSSignature` doesn't implement methods like `to_bytes`,
/// which means we need to use bincode to serialize/deserialize it.
/// However, using bincode's default config (little-endian + variable int encoding)
/// add extra bytes to the serialized output, which is not what we want.
/// Thus, define a custom configuration for bincode here.
const BINCODE_CONFIG: bincode::config::Configuration<LittleEndian, Fixint, NoLimit> =
    bincode::config::standard().with_fixed_int_encoding();
