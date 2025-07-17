use std::num::NonZeroUsize;

use lru::LruCache;
use ream_bls::{BLSSignature, PublicKey};
use ream_consensus_beacon::bls_to_execution_change::BLSToExecutionChange;
use tokio::sync::RwLock;

const LRU_CACHE_SIZE: usize = 64;

#[derive(Debug, Hash, PartialEq, Eq, Default, Clone)]
pub struct AddressSlotIdentifier {
    pub address: PublicKey,
    pub slot: u64,
}

/// In-memory LRU cache.
#[derive(Debug)]
pub struct CachedDB {
    pub cached_proposer_signature: RwLock<LruCache<AddressSlotIdentifier, BLSSignature>>,
    pub cached_bls_to_execution_signature:
        RwLock<LruCache<AddressSlotIdentifier, BLSToExecutionChange>>,
    pub cached_seen_blob_sidecars: RwLock<LruCache<(u64, u64, u64), ()>>,
}

impl CachedDB {
    pub fn new() -> Self {
        Self {
            cached_proposer_signature: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            cached_bls_to_execution_signature: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            cached_seen_blob_sidecars: LruCache::new(NonZeroUsize::new(LRU_CACHE_SIZE).unwrap())
                .into(),
        }
    }
}

impl Default for CachedDB {
    fn default() -> Self {
        Self::new()
    }
}
