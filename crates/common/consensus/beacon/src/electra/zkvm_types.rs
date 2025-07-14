#[cfg(feature = "zkvm")]
use ssz_types::typenum::U536870912;
#[cfg(not(feature = "zkvm"))]
use ssz_types::typenum::U1099511627776;

// VALIDATOR_REGISTRY_LIMIT
#[cfg(not(feature = "zkvm"))]
pub type ValidatorRegistryLimit = U1099511627776;
#[cfg(feature = "zkvm")]
pub type ValidatorRegistryLimit = U536870912;
