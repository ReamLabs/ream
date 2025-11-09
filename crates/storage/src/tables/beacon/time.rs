use std::sync::Arc;

use redb::{Database, TableDefinition};

use crate::tables::field::REDBField;

pub struct TimeField {
    pub db: Arc<Database>,
}

/// Table definition for the Time table
///
/// Value: u64
impl REDBField for TimeField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, u64> = TableDefinition::new("beacon_time");

    const KEY: &str = "time_key";

    type Value = u64;

    type ValueFieldDefinition = u64;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
