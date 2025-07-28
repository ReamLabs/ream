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

#[derive(Debug, Hash, Eq, PartialEq, Default)]
pub struct AtestationKey {
    pub attestation_subnet_id: u64,
    pub target_epoch: u64,
    pub participating_validator_index: u64,
}

#[derive(Debug, Hash, Eq, PartialEq, Default)]
pub struct AddressValidaterIndexIdentifier {
    pub address: PublicKey,
    pub validator_index: u64,
}

#[derive(Debug, Hash, Eq, PartialEq, Default, Clone)]
pub struct SyncCommitteeKey {
    pub subnet_id: u64,
    pub slot: u64,
    pub validator_index: u64,
}

/// In-memory LRU cache.
#[derive(Debug)]
pub struct CachedDB {
    pub proposer_signature: RwLock<LruCache<AddressSlotIdentifier, BLSSignature>>,
    pub bls_to_execution_signature: RwLock<LruCache<AddressSlotIdentifier, BLSToExecutionChange>>,
    pub seen_blob_sidecars: RwLock<LruCache<(u64, u64, u64), ()>>,
    pub seen_attestations: RwLock<LruCache<AtestationKey, ()>>,
    pub seen_bls_to_execution_change: RwLock<LruCache<AddressValidaterIndexIdentifier, ()>>,
    pub seen_sync_messages: RwLock<LruCache<SyncCommitteeKey, ()>>,
    pub seen_voluntary_exit: RwLock<LruCache<u64, ()>>,
    pub seen_proposer_slashings: RwLock<LruCache<u64, ()>>,
    pub prior_seen_attester_slashing_indices: RwLock<LruCache<u64, ()>>,
}

impl CachedDB {
    pub fn new() -> Self {
        Self {
            proposer_signature: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            bls_to_execution_signature: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_blob_sidecars: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_attestations: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_bls_to_execution_change: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_sync_messages: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_voluntary_exit: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_proposer_slashings: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            prior_seen_attester_slashing_indices: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
        }
    }
}

impl Default for CachedDB {
    fn default() -> Self {
        Self::new()
    }
}
