use std::{collections::HashMap, sync::Arc};

use ream_consensus_lean::attestation::{AggregatedSignatureProof, SignatureKey};
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanLatestNewAggregatedPayloadsTable {
    pub db: Arc<Database>,
}

/// Table definition for the Lean Latest New Aggregated Payloads table
///
/// Key: SignatureKey
/// Value: [AggregatedSignatureProof]
impl REDBTable for LeanLatestNewAggregatedPayloadsTable {
    const TABLE_DEFINITION: TableDefinition<
        'static,
        SSZEncoding<SignatureKey>,
        SSZEncoding<Vec<AggregatedSignatureProof>>,
    > = TableDefinition::new("lean_latest_new_aggregated_payloads");

    type Key = SignatureKey;

    type KeyTableDefinition = SSZEncoding<SignatureKey>;

    type Value = Vec<AggregatedSignatureProof>;

    type ValueTableDefinition = SSZEncoding<Vec<AggregatedSignatureProof>>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn get<'a>(&self, key: SignatureKey) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.database().begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let result = table.get(key)?;
        Ok(result.map(|guard| guard.value()))
    }

    fn insert(
        &self,
        key: SignatureKey,
        value: Vec<AggregatedSignatureProof>,
    ) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }
}

impl LeanLatestNewAggregatedPayloadsTable {
    pub fn iter(&self) -> Result<Vec<(SignatureKey, Vec<AggregatedSignatureProof>)>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

        let mut entries = Vec::new();
        for result in table.iter()? {
            let (key_guard, value_guard) = result?;

            let key: SignatureKey = key_guard.value();
            let value: Vec<AggregatedSignatureProof> = value_guard.value();

            entries.push((key, value));
        }
        Ok(entries)
    }

    pub fn retain<F>(&self, mut f: F) -> Result<(), StoreError>
    where
        F: FnMut(&SignatureKey, &Vec<AggregatedSignatureProof>) -> bool,
    {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;

            table.retain(|key, value| {
                let key_ref: &SignatureKey = &key;
                let val_ref: &Vec<AggregatedSignatureProof> = &value;

                f(key_ref, val_ref)
            })?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn drain(
        &self,
    ) -> Result<HashMap<SignatureKey, Vec<AggregatedSignatureProof>>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;

        let mut result = HashMap::new();
        while let Some((key, value)) = table.pop_first()? {
            result.insert(key.value(), value.value());
        }
        drop(table);
        write_txn.commit()?;
        Ok(result)
    }
}
