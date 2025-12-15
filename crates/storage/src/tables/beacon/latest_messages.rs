use std::sync::Arc;

use ream_consensus_beacon::fork_choice::latest_message::LatestMessage;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct LatestMessagesTable {
    pub db: Arc<Database>,
}

/// Table definition for the Latest Message table
///
/// Key: latest_messages
/// Value: LatestMessage
impl REDBTable for LatestMessagesTable {
    const TABLE_DEFINITION: TableDefinition<'_, u64, SSZEncoding<LatestMessage>> =
        TableDefinition::new("beacon_latest_messages");

    type Key = u64;

    type KeyTableDefinition = u64;

    type Value = LatestMessage;

    type ValueTableDefinition = SSZEncoding<LatestMessage>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
