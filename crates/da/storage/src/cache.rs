use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use ream_consensus_beacon::data_column_sidecar::DataColumnSidecar;
use tokio::sync::RwLock;

/// In-memory cache for recently accessed column sidecars.
///
/// Keyed by (block_root, column_index). Values are Arc-wrapped so callers
/// get a cheap clone of the pointer rather than a 131 KB blob copy.
///
/// The cache is intentionally unbounded — the DA node custodies all 128
/// columns per block, and the retention window is pruned by the consensus
/// loop. At ~131 KB per column × 128 columns × ~4096 slots retained, the
/// theoretical max is ~68 GB, so this is a hot-block cache only, not a
/// full retention cache. Callers should insert on write and remove on prune.
pub struct DaColumnCache {
    inner: RwLock<HashMap<(B256, u64), Arc<DataColumnSidecar>>>,
}

impl DaColumnCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a sidecar into the cache, wrapping it in Arc.
    pub async fn insert(&self, block_root: B256, index: u64, sidecar: DataColumnSidecar) {
        let mut guard = self.inner.write().await;
        guard.insert((block_root, index), Arc::new(sidecar));
    }

    /// Get a single column. Returns a cheap Arc clone — no blob copy.
    pub async fn get(&self, block_root: B256, index: u64) -> Option<Arc<DataColumnSidecar>> {
        let guard = self.inner.read().await;
        guard.get(&(block_root, index)).cloned()
    }

    /// Get all held columns for a block root without 128 sequential file reads.
    /// Returns only entries that are present — holes are skipped.
    pub async fn get_all_for_block(&self, block_root: B256) -> Vec<Arc<DataColumnSidecar>> {
        let guard = self.inner.read().await;
        // Single lock acquisition, single pass — O(cache_entries_for_block)
        // rather than O(128) file opens.
        guard
            .iter()
            .filter_map(|((root, _), sidecar)| {
                if *root == block_root {
                    Some(sidecar.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// How many columns are cached for a given block root.
    pub async fn count(&self, block_root: B256) -> usize {
        let guard = self.inner.read().await;
        guard.keys().filter(|(root, _)| *root == block_root).count()
    }

    /// Remove a single entry — called by prune.
    pub async fn remove(&self, block_root: B256, index: u64) {
        let mut guard = self.inner.write().await;
        guard.remove(&(block_root, index));
    }

    /// Remove all entries for a block root — called by prune.
    pub async fn remove_all_for_block(&self, block_root: B256) {
        let mut guard = self.inner.write().await;
        guard.retain(|(root, _), _| *root != block_root);
    }
}

impl Default for DaColumnCache {
    fn default() -> Self {
        Self::new()
    }
}
