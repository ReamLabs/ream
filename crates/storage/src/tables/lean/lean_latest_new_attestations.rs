use std::{collections::HashMap, sync::Arc};

use ream_consensus_lean::attestation::SignedAttestation;
use redb::{Database, ReadableDatabase, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanLatestNewAttestationsTable {
    pub db: Arc<Database>,
}

/// Table definition for the Latest New Attestations table
///
/// Key: u64
/// Value: SignedAttestation
impl REDBTable for LeanLatestNewAttestationsTable {
    const TABLE_DEFINITION: TableDefinition<'_, u64, SSZEncoding<SignedAttestation>> =
        TableDefinition::new("lean_latest_new_attestations");

    type Key = u64;

    type KeyTableDefinition = u64;

    type Value = SignedAttestation;

    type ValueTableDefinition = SSZEncoding<SignedAttestation>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl LeanLatestNewAttestationsTable {
    pub fn iter_values(
        &self,
    ) -> Result<impl Iterator<Item = anyhow::Result<SignedAttestation>>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table
            .range::<<u64 as redb::Value>::SelfType<'_>>(..)?
            .map(|result| {
                result
                    .map(|(_, value)| value.value())
                    .map_err(|err| StoreError::from(err).into())
            }))
    }

    pub fn drain(&self) -> Result<HashMap<u64, SignedAttestation>, StoreError> {
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
