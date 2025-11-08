use std::{collections::HashMap, sync::Arc};

use ream_consensus_lean::attestation::SignedAttestation;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LatestKnownAttestationTable {
    pub db: Arc<Database>,
}

/// Table definition for the Latest Known Attestation table
///
/// Key: u64 (validator index)
/// Value: [SignedAttestation]
impl REDBTable for LatestKnownAttestationTable {
    const TABLE_DEFINITION: TableDefinition<'_, u64, SSZEncoding<SignedAttestation>> =
        TableDefinition::new("latest_known_attestation");

    type Key = u64;

    type KeyTableDefinition = u64;

    type Value = SignedAttestation;

    type ValueTableDefinition = SSZEncoding<SignedAttestation>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl LatestKnownAttestationTable {
    /// Insert multiple attestations with validator id in a single transaction.
    pub fn batch_insert(
        &self,
        values: impl IntoIterator<Item = (u64, SignedAttestation)>,
    ) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;

        let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;

        for (key, value) in values {
            table.insert(key, value)?;
        }

        drop(table);
        write_txn.commit()?;

        Ok(())
    }

    /// Get all attestations.
    pub fn get_all_attestations(&self) -> Result<HashMap<u64, SignedAttestation>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

        table
            .iter()?
            .map(|entry| {
                let (k, v) = entry?;
                Ok((k.value(), v.value()))
            })
            .collect()
    }
}
