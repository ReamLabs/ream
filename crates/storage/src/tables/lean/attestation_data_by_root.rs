use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::attestation::AttestationData;
use redb::{
    Database, Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanAttestationDataByRootTable {
    pub db: Arc<Database>,
}

/// Table definition for the Lean Attestation Data By Root table
///
/// Key: B256
/// Value: [AttestationData]
impl REDBTable for LeanAttestationDataByRootTable {
    const TABLE_DEFINITION: TableDefinition<
        'static,
        SSZEncoding<B256>,
        SSZEncoding<AttestationData>,
    > = TableDefinition::new("lean_attestation_data_by_root");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = AttestationData;

    type ValueTableDefinition = SSZEncoding<AttestationData>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn get<'a>(&self, key: B256) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.database().begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let result = table.get(key)?;
        Ok(result.map(|guard| guard.value()))
    }
}

impl LeanAttestationDataByRootTable {
    pub fn iter(&self) -> Result<Vec<(B256, AttestationData)>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

        let mut entries = Vec::new();
        for result in table.iter()? {
            let (key_guard, value_guard) = result?;

            let key: B256 = key_guard.value();
            let value: AttestationData = value_guard.value();

            entries.push((key, value));
        }
        Ok(entries)
    }

    pub fn retain<F>(&self, mut f: F) -> Result<(), StoreError>
    where
        F: FnMut(&B256, &AttestationData) -> bool,
    {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;

            table.retain(|key, value| {
                let key_ref: &B256 = &key;
                let value_ref: &AttestationData = &value;

                f(key_ref, value_ref)
            })?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn contains_key(&self, key: &B256) -> bool {
        self.get(*key)
            .map(|option| option.is_some())
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        let Ok(read_txn) = self.db.begin_read() else {
            return 0;
        };
        let Ok(table) = read_txn.open_table(Self::TABLE_DEFINITION) else {
            return 0;
        };
        table.len().unwrap_or(0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
