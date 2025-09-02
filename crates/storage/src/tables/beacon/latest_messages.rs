use std::sync::Arc;

use ream_consensus_beacon::fork_choice::latest_message::LatestMessage;
use redb::{Database, Durability, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the Latest Message table
///
/// Key: latest_messages
/// Value: LatestMessage
pub(crate) const LATEST_MESSAGES_TABLE: TableDefinition<u64, SSZEncoding<LatestMessage>> =
    TableDefinition::new("beacon_latest_messages");

pub struct LatestMessagesTable {
    pub db: Arc<Database>,
}

impl Table for LatestMessagesTable {
    type Key = u64;

    type Value = LatestMessage;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LATEST_MESSAGES_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(LATEST_MESSAGES_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
