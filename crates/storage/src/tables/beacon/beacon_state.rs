use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct BeaconStateTable {
    pub db: Arc<Database>,
}

/// Table definition for the Beacon State table
///
/// Key: block_root
/// Value: BeaconState
impl REDBTable for BeaconStateTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<BeaconState>> =
        TableDefinition::new("beacon_state");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = BeaconState;

    type ValueTableDefinition = SSZEncoding<BeaconState>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
