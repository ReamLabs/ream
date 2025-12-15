use std::sync::Arc;

use redb::{Database, TableDefinition};

use crate::tables::field::REDBField;

pub struct LeanTimeField {
    pub db: Arc<Database>,
}

/// Table definition for the Lean Time table
///
/// Value: u64
impl REDBField for LeanTimeField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, u64> = TableDefinition::new("lean_time");

    const KEY: &str = "lean_time_key";

    type Value = u64;

    type ValueFieldDefinition = u64;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
