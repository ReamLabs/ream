pub const ATTESTATION_COMMITTEE_COUNT: u64 = 1;

#[cfg(feature = "devnet2")]
pub const INTERVALS_PER_SLOT: u64 = 4;
#[cfg(feature = "devnet3")]
pub const INTERVALS_PER_SLOT: u64 = 5;
pub const MAX_HISTORICAL_BLOCK_HASHES: u64 = 262144;
pub const SLOT_DURATION: u64 = 4;
pub const VALIDATOR_REGISTRY_LIMIT: u64 = 4096;
