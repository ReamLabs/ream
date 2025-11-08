use std::sync::Arc;

use redb::{Database, TableDefinition};

use crate::tables::field::REDBField;

pub struct GenesisTimeField {
    pub db: Arc<Database>,
}

/// Table definition for the Genesis_Time table
///
/// Value: u64
impl REDBField for GenesisTimeField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, u64> =
        TableDefinition::new("beacon_genesis_time");

    const KEY: &str = "genesis_time_key";

    type Value = u64;

    type ValueFieldDefinition = u64;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
