use std::{collections::HashMap, sync::Arc};

use ream_consensus_lean::attestation::SignatureKey;
#[cfg(feature = "devnet5")]
use ream_consensus_lean::attestation::SingleMessageAggregate as PayloadProof;
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
/// Value: [PayloadProof] (a SingleMessageAggregate on devnet5)
impl REDBTable for LeanLatestNewAggregatedPayloadsTable {
    const TABLE_DEFINITION: TableDefinition<
        'static,
        SSZEncoding<SignatureKey>,
        SSZEncoding<Vec<PayloadProof>>,
    > = TableDefinition::new("lean_latest_new_aggregated_payloads");

    type Key = SignatureKey;

    type KeyTableDefinition = SSZEncoding<SignatureKey>;

    type Value = Vec<PayloadProof>;

    type ValueTableDefinition = SSZEncoding<Vec<PayloadProof>>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn get<'a>(&self, key: SignatureKey) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.database().begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let result = table.get(key)?;
        Ok(result.map(|guard| guard.value()))
    }

    fn insert(&self, key: SignatureKey, value: Vec<PayloadProof>) -> Result<(), StoreError> {
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
    pub fn iter(&self) -> Result<Vec<(SignatureKey, Vec<PayloadProof>)>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

        let mut entries = Vec::new();
        for result in table.iter()? {
            let (key_guard, value_guard) = result?;

            let key: SignatureKey = key_guard.value();
            let value: Vec<PayloadProof> = value_guard.value();

            entries.push((key, value));
        }
        Ok(entries)
    }

    pub fn retain<F>(&self, mut f: F) -> Result<(), StoreError>
    where
        F: FnMut(&SignatureKey, &Vec<PayloadProof>) -> bool,
    {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;

            table.retain(|key, value| {
                let key_ref: &SignatureKey = &key;
                let val_ref: &Vec<PayloadProof> = &value;

                f(key_ref, val_ref)
            })?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn drain(&self) -> Result<HashMap<SignatureKey, Vec<PayloadProof>>, StoreError> {
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

    pub fn contains_key(&self, key: &SignatureKey) -> bool {
        self.get(key.clone())
            .map(|option| option.is_some())
            .unwrap_or(false)
    }

    pub fn get_all(&self) -> Result<HashMap<SignatureKey, Vec<PayloadProof>>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

        let mut map = HashMap::new();
        for result in table.iter()? {
            let (key_guard, value_guard) = result?;
            map.insert(key_guard.value(), value_guard.value());
        }
        Ok(map)
    }

    pub fn update_all(
        &self,
        updates: HashMap<SignatureKey, Vec<PayloadProof>>,
    ) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            for (key, value) in updates {
                table.insert(key, value)?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }
}
