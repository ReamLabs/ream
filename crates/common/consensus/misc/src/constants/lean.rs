/// 3SF-mini divides a slot into 4 intervals.
/// Reference: https://github.com/ethereum/research/blob/d225a6775a9b184b5c1fd6c830cc58a375d9535f/3sf-mini/p2p.py#L77-L98
pub const INTERVALS_PER_SLOT: u64 = 4;
pub const MAX_HISTORICAL_BLOCK_HASHES: u64 = 262144;
pub const SLOT_DURATION: u64 = 12;
pub const SLOT_OFFSET: u64 = 1;
pub const VALIDATOR_REGISTRY_LIMIT: u64 = 4096;
