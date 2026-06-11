use std::{path::PathBuf, sync::Arc};

use alloy_primitives::B256;
use ream_da_errors::{DaError, DaResult};
use redb::{Database, ReadableDatabase, TableDefinition};

const SLOT_INDEX_TABLE: TableDefinition<u64, [u8; 32]> = TableDefinition::new("da_slot_index");

pub struct DaSlotIndex {
    db: Arc<Database>,
}

impl DaSlotIndex {
    pub fn new(data_dir: PathBuf) -> DaResult<Self> {
        let db = Database::create(data_dir.join("da_slot_index.redb"))
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;

        let txn = db
            .begin_write()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        txn.open_table(SLOT_INDEX_TABLE)
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        txn.commit()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;

        Ok(Self { db: Arc::new(db) })
    }

    pub fn put(&self, slot: u64, block_root: B256) -> DaResult<()> {
        let txn = self
            .db
            .begin_write()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        {
            let mut table = txn
                .open_table(SLOT_INDEX_TABLE)
                .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
            table
                .insert(slot, block_root.0)
                .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        }
        txn.commit()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        Ok(())
    }

    pub fn get(&self, slot: u64) -> DaResult<Option<B256>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        let table = txn
            .open_table(SLOT_INDEX_TABLE)
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        Ok(table
            .get(slot)
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?
            .map(|v| B256::from(v.value())))
    }

    pub fn get_range(&self, start_slot: u64, count: u64) -> DaResult<Vec<(u64, B256)>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        let table = txn
            .open_table(SLOT_INDEX_TABLE)
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;

        table
            .range(start_slot..start_slot + count)
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?
            .map(|item| {
                let (slot, root) = item.map_err(|e| DaError::SlotIndexFailed(e.into()))?;
                Ok((slot.value(), B256::from(root.value())))
            })
            .collect()
    }

    pub fn get_slots_before(&self, min_slot: u64) -> DaResult<Vec<(u64, B256)>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        let table = txn
            .open_table(SLOT_INDEX_TABLE)
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;

        table
            .range(..min_slot)
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?
            .map(|item| {
                let (slot, root) = item.map_err(|e| DaError::SlotIndexFailed(e.into()))?;
                Ok((slot.value(), B256::from(root.value())))
            })
            .collect()
    }

    pub fn remove(&self, slot: u64) -> DaResult<()> {
        let txn = self
            .db
            .begin_write()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        {
            let mut table = txn
                .open_table(SLOT_INDEX_TABLE)
                .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
            table
                .remove(slot)
                .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        }
        txn.commit()
            .map_err(|e| DaError::SlotIndexFailed(e.into()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempdir::TempDir;

    #[test]
    fn test_put_and_get() {
        let tmp = TempDir::new("da_slot").unwrap();
        let index = DaSlotIndex::new(tmp.path().to_path_buf()).unwrap();

        index.put(100, B256::ZERO).unwrap();
        assert_eq!(index.get(100).unwrap(), Some(B256::ZERO));
        assert_eq!(index.get(101).unwrap(), None);
    }

    #[test]
    fn test_get_range() {
        let tmp = TempDir::new("da_range").unwrap();
        let index = DaSlotIndex::new(tmp.path().to_path_buf()).unwrap();

        for slot in 100..110 {
            index.put(slot, B256::ZERO).unwrap();
        }

        let range = index.get_range(102, 5).unwrap();
        assert_eq!(range.len(), 5);
        assert_eq!(range[0].0, 102);
        assert_eq!(range[4].0, 106);
    }

    #[test]
    fn test_get_slots_before() {
        let tmp = TempDir::new("da_before").unwrap();
        let index = DaSlotIndex::new(tmp.path().to_path_buf()).unwrap();

        for slot in 100..110 {
            index.put(slot, B256::ZERO).unwrap();
        }

        let stale = index.get_slots_before(105).unwrap();
        assert_eq!(stale.len(), 5);
    }
}
