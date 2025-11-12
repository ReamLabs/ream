use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::state::LeanState;
use redb::{Database, ReadableDatabase, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanStateTable {
    pub db: Arc<Database>,
}

/// Table definition for the Lean State table
///
/// Key: block_root
/// Value: [LeanState]
impl REDBTable for LeanStateTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<LeanState>> =
        TableDefinition::new("lean_state");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = LeanState;

    type ValueTableDefinition = SSZEncoding<LeanState>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl LeanStateTable {
    pub fn iter_values(
        &self,
    ) -> Result<impl Iterator<Item = anyhow::Result<LeanState>>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table
            .range::<<SSZEncoding<B256> as redb::Value>::SelfType<'_>>(..)?
            .map(|result| {
                result
                    .map(|(_, value)| value.value())
                    .map_err(|err| StoreError::from(err).into())
            }))
    }
}
