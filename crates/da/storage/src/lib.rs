pub mod cache;
pub mod column;
pub mod prune;
pub mod slot_index;

pub use cache::DaColumnCache;
pub use column::DaColumnStore;
pub use slot_index::DaSlotIndex;

use std::{collections::HashSet, path::PathBuf, sync::Arc};

use alloy_primitives::B256;
use ream_consensus_beacon::data_column_sidecar::{DataColumnSidecar, NUMBER_OF_COLUMNS};
use ream_da_errors::{DaError, DaResult};

pub struct DaStore {
    pub columns: DaColumnStore,
    pub slot_index: DaSlotIndex,
    cache: DaColumnCache,
}

impl DaStore {
    pub fn new(data_dir: PathBuf) -> DaResult<Self> {
        std::fs::create_dir_all(&data_dir).map_err(|e| DaError::Internal(e.to_string()))?;
        Ok(Self {
            columns: DaColumnStore::new(data_dir.clone())?,
            slot_index: DaSlotIndex::new(data_dir)?,
            cache: DaColumnCache::new(),
        })
    }

    /// Store a validated column sidecar — writes to disk and populates cache.
    pub async fn put_column(&self, block_root: B256, sidecar: DataColumnSidecar) -> DaResult<()> {
        let index = sidecar.index;
        self.columns
            .insert(block_root, sidecar.clone())
            .map_err(|e| DaError::ColumnWriteFailed {
                block_root: block_root.to_string(),
                index,
                source: e,
            })?;
        self.cache.insert(block_root, index, sidecar).await;
        Ok(())
    }

    pub fn record_slot(&self, slot: u64, block_root: B256) -> DaResult<()> {
        self.slot_index.put(slot, block_root)
    }

    /// Get a single column as `Arc` — cache-first, disk fallback.
    /// Preferred for reconstruction and gossip paths where cloning is expensive.
    pub async fn get_column(
        &self,
        block_root: B256,
        index: u64,
    ) -> DaResult<Option<Arc<DataColumnSidecar>>> {
        if let Some(cached) = self.cache.get(block_root, index).await {
            return Ok(Some(cached));
        }
        let sidecar =
            self.columns
                .get(block_root, index)
                .map_err(|e| DaError::ColumnReadFailed {
                    block_root: block_root.to_string(),
                    index,
                    source: e,
                })?;
        if let Some(sidecar) = sidecar {
            self.cache.insert(block_root, index, sidecar.clone()).await;
            return Ok(Some(Arc::new(sidecar)));
        }
        Ok(None)
    }

    /// Get a single column as an owned value — for req/resp handlers that
    /// need to pass `DataColumnSidecar` directly into `BeaconResponseMessage`.
    ///
    /// Uses `Arc::unwrap_or_clone`: free move-out when refcount is 1 (the
    /// common case immediately after a cache miss), clone only when shared.
    pub async fn get_column_owned(
        &self,
        block_root: B256,
        index: u64,
    ) -> DaResult<Option<DataColumnSidecar>> {
        Ok(self
            .get_column(block_root, index)
            .await?
            .map(Arc::unwrap_or_clone))
    }

    /// Count — cache-first (avoids dir scan when cache is warm).
    pub async fn column_count(&self, block_root: B256) -> usize {
        let cached = self.cache.count(block_root).await;
        if cached > 0 {
            return cached;
        }
        self.columns.count(block_root)
    }

    /// Get all columns as `Arc` — cache-first, disk fallback for missing indices.
    ///
    /// Once gossip has delivered all 128 columns this is a single RwLock
    /// acquisition + Vec allocation instead of 128 sequential file opens.
    pub async fn get_all_columns(&self, block_root: B256) -> DaResult<Vec<Arc<DataColumnSidecar>>> {
        let cached = self.cache.get_all_for_block(block_root).await;

        if cached.len() == NUMBER_OF_COLUMNS as usize {
            return Ok(cached);
        }

        let cached_indices: HashSet<u64> = cached.iter().map(|s| s.index).collect();
        let mut result = cached;

        for i in 0..NUMBER_OF_COLUMNS {
            if cached_indices.contains(&i) {
                continue;
            }
            let sidecar =
                self.columns
                    .get(block_root, i)
                    .map_err(|e| DaError::ColumnReadFailed {
                        block_root: block_root.to_string(),
                        index: i,
                        source: e,
                    })?;
            if let Some(sidecar) = sidecar {
                self.cache.insert(block_root, i, sidecar.clone()).await;
                result.push(Arc::new(sidecar));
            }
        }

        Ok(result)
    }

    pub fn block_root_for_slot(&self, slot: u64) -> DaResult<Option<B256>> {
        self.slot_index.get(slot)
    }

    pub fn slots_in_range(&self, start_slot: u64, count: u64) -> DaResult<Vec<(u64, B256)>> {
        self.slot_index.get_range(start_slot, count)
    }

    /// Prune — removes from disk, slot index, and cache.
    pub async fn prune_before_slot(&self, min_slot: u64) -> DaResult<usize> {
        prune::prune_before_slot(&self.columns, &self.slot_index, &self.cache, min_slot).await
    }

    pub fn get_columns_for_block(
        &self,
        block_root: B256,
        indices: &[u64],
    ) -> DaResult<Vec<DataColumnSidecar>> {
        indices
            .iter()
            .filter_map(|&index| {
                self.columns
                    .get(block_root, index)
                    .map_err(|e| DaError::ColumnReadFailed {
                        block_root: block_root.to_string(),
                        index: index,
                        source: e,
                    })
                    .transpose()
            })
            .collect()
    }
}
