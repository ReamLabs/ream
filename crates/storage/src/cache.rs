use std::num::NonZeroUsize;

use lru::LruCache;
use ream_bls::{BLSSignature, PublicKey};
use ream_consensus::bls_to_execution_change::BLSToExecutionChange;
use tokio::sync::RwLock;

/// In-memory LRU cache.
#[derive(Debug)]
pub struct CachedDB {
    pub cached_proposer_signature: RwLock<LruCache<(PublicKey, u64), BLSSignature>>,
    pub cached_bls_to_execution_signature: RwLock<LruCache<(PublicKey, u64), BLSToExecutionChange>>,
}

impl CachedDB {
    pub fn new() -> Self {
        Self {
            cached_proposer_signature: LruCache::new(unsafe { NonZeroUsize::new_unchecked(64) })
                .into(),
            cached_bls_to_execution_signature: LruCache::new(unsafe {
                NonZeroUsize::new_unchecked(64)
            })
            .into(),
        }
    }
}

impl Default for CachedDB {
    fn default() -> Self {
        Self::new()
    }
}
