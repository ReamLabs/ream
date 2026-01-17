use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, TableDefinition};

use crate::tables::{field::REDBField, ssz_encoder::SSZEncoding};

pub struct LeanSafeTargetField {
    pub db: Arc<Database>,
}

/// Table definition for the Lean Safe Target table
///
/// Value: B256
impl REDBField for LeanSafeTargetField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, SSZEncoding<B256>> =
        TableDefinition::new("lean_safe_target");

    const KEY: &str = "lean_safe_target_key";

    type Value = B256;

    type ValueFieldDefinition = SSZEncoding<B256>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
