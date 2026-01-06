use std::{num::NonZeroUsize, sync::Mutex};

use alloy_primitives::{B256, FixedBytes};
use lru::LruCache;
use ream_bls::{BLSSignature, PublicKey};
use ream_consensus_beacon::{
    bls_to_execution_change::BLSToExecutionChange,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
};
use ream_consensus_lean::{block::SignedBlockWithAttestation, state::LeanState};
use ream_consensus_misc::constants::beacon::SYNC_COMMITTEE_SIZE;
use ream_light_client::finality_update::LightClientFinalityUpdate;
use tokio::sync::RwLock;
const LRU_CACHE_SIZE: usize = 64;
const BLOCK_CACHE_SIZE: usize = 128;
const STATE_CACHE_SIZE: usize = 8;

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

#[derive(Debug, Hash, Eq, PartialEq, Default, Clone)]
pub struct CacheSyncCommitteeContribution {
    pub slot: u64,
    pub beacon_block_root: FixedBytes<32>,
    pub subcommittee_index: u64,
}

#[derive(Debug, Hash, Eq, PartialEq, Default, Clone)]
pub struct AggregateAndProofKey {
    pub aggregator_index: u64,
    pub target_epoch: u64,
}

/// In-memory LRU cache for beacon node (gossip validation + beacon storage).
#[derive(Debug)]
pub struct BeaconCacheDB {
    // Gossip validation caches
    pub seen_proposer_signature: RwLock<LruCache<AddressSlotIdentifier, BLSSignature>>,
    pub seen_bls_to_execution_signature:
        RwLock<LruCache<AddressSlotIdentifier, BLSToExecutionChange>>,
    pub seen_blob_sidecars: RwLock<LruCache<(u64, u64, u64), ()>>,
    pub seen_data_column_sidecars: RwLock<LruCache<(u64, u64, u64), ()>>,
    pub seen_attestations: RwLock<LruCache<AtestationKey, ()>>,
    pub seen_bls_to_execution_change: RwLock<LruCache<AddressValidaterIndexIdentifier, ()>>,
    pub seen_sync_messages: RwLock<LruCache<SyncCommitteeKey, ()>>,
    pub seen_sync_committee_contributions: RwLock<LruCache<CacheSyncCommitteeContribution, ()>>,
    pub seen_forwarded_finality_update_slot: RwLock<Option<(u64, bool)>>,
    pub seen_voluntary_exit: RwLock<LruCache<u64, ()>>,
    pub seen_proposer_slashings: RwLock<LruCache<u64, ()>>,
    pub prior_seen_attester_slashing_indices: RwLock<LruCache<u64, ()>>,
    pub forwarded_optimistic_update_slot: RwLock<Option<u64>>,
    pub forwarded_light_client_finality_update: RwLock<Option<LightClientFinalityUpdate>>,
    pub seen_aggregate_and_proof: RwLock<LruCache<AggregateAndProofKey, ()>>,
    // Beacon storage caches
    pub blocks: Mutex<LruCache<B256, SignedBeaconBlock>>,
    pub states: Mutex<LruCache<B256, BeaconState>>,
}

impl BeaconCacheDB {
    pub fn new() -> Self {
        Self {
            seen_proposer_signature: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_bls_to_execution_signature: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_blob_sidecars: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            seen_data_column_sidecars: LruCache::new(
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
            seen_sync_committee_contributions: LruCache::new(
                NonZeroUsize::new(SYNC_COMMITTEE_SIZE as usize).expect("Invalid cache size"),
            )
            .into(),
            seen_forwarded_finality_update_slot: RwLock::new(None),
            forwarded_optimistic_update_slot: None.into(),
            forwarded_light_client_finality_update: None.into(),
            seen_aggregate_and_proof: LruCache::new(
                NonZeroUsize::new(LRU_CACHE_SIZE).expect("Invalid cache size"),
            )
            .into(),
            blocks: Mutex::new(LruCache::new(
                NonZeroUsize::new(BLOCK_CACHE_SIZE).expect("Invalid cache size"),
            )),
            states: Mutex::new(LruCache::new(
                NonZeroUsize::new(STATE_CACHE_SIZE).expect("Invalid cache size"),
            )),
        }
    }
}

impl Default for BeaconCacheDB {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory LRU cache for lean node (lean storage only).
#[derive(Debug)]
pub struct LeanCacheDB {
    // Lean storage caches
    pub blocks: Mutex<LruCache<B256, SignedBlockWithAttestation>>,
    pub states: Mutex<LruCache<B256, LeanState>>,
}

impl LeanCacheDB {
    pub fn new() -> Self {
        Self {
            blocks: Mutex::new(LruCache::new(
                NonZeroUsize::new(BLOCK_CACHE_SIZE).expect("Invalid cache size"),
            )),
            states: Mutex::new(LruCache::new(
                NonZeroUsize::new(STATE_CACHE_SIZE).expect("Invalid cache size"),
            )),
        }
    }
}

impl Default for LeanCacheDB {
    fn default() -> Self {
        Self::new()
    }
}
