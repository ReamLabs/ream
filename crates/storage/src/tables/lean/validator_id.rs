use std::sync::Arc;

use redb::{Database, TableDefinition};

use crate::tables::field::REDBField;

pub struct LeanValidatorIdField {
    pub db: Arc<Database>,
}

/// Table definition for the Lean Validator Id table
///
/// Value: u64
impl REDBField for LeanValidatorIdField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, Option<u64>> =
        TableDefinition::new("lean_validator_id");

    const KEY: &str = "lean_validator_id";

    type Value = Option<u64>;

    type ValueFieldDefinition = Option<u64>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
