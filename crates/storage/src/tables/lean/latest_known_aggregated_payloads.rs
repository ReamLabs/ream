use std::sync::Arc;

use ream_consensus_lean::attestation::{AggregatedSignatureProof, SignatureKey};
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanLatestKnownAggregatedPayloadsTable {
    pub db: Arc<Database>,
}

/// Table definition for the Lean Latest Known Aggregated Payloads table
///
/// Key: SignatureKey
/// Value: [AggregatedSignatureProof]
impl REDBTable for LeanLatestKnownAggregatedPayloadsTable {
    const TABLE_DEFINITION: TableDefinition<
        'static,
        SSZEncoding<SignatureKey>,
        SSZEncoding<Vec<AggregatedSignatureProof>>,
    > = TableDefinition::new("lean_latest_known_aggregated_payloads");

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

impl LeanLatestKnownAggregatedPayloadsTable {
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
                let value_ref: &Vec<AggregatedSignatureProof> = &value;

                f(key_ref, value_ref)
            })?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn contains_key(&self, key: &SignatureKey) -> bool {
        self.get(key.clone())
            .map(|option| option.is_some())
            .unwrap_or(false)
    }
}
