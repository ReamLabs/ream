use tracing::info;

use ream_da_errors::{DaError, DaResult};

use crate::{DaColumnCache, DaColumnStore, DaSlotIndex};

/// Prune all column data for slots older than `min_slot`.
///
/// Removes from three places in order:
/// 1. Column files on disk (`DaColumnStore`)
/// 2. Slot → block_root index (`DaSlotIndex`)
/// 3. In-memory cache (`DaColumnCache`)
///
/// Called every epoch with:
///   `finalized_slot - MIN_EPOCHS_FOR_DATA_COLUMN_SIDECARS_REQUESTS * 32`
pub async fn prune_before_slot(
    columns: &DaColumnStore,
    slot_index: &DaSlotIndex,
    cache: &DaColumnCache,
    min_slot: u64,
) -> DaResult<usize> {
    let stale = slot_index.get_slots_before(min_slot)?;
    let mut total_removed = 0;

    for (slot, block_root) in stale {
        let removed = columns
            .remove_all_for_block(block_root)
            .map_err(|e| DaError::SlotIndexFailed(e))?;

        slot_index.remove(slot)?;
        cache.remove_all_for_block(block_root).await;

        total_removed += removed;

        if removed > 0 {
            info!(
                slot,
                %block_root,
                columns_removed = removed,
                "Pruned stale column data"
            );
        }
    }

    Ok(total_removed)
}
