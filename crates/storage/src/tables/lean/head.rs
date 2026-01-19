use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, TableDefinition};

use crate::tables::{field::REDBField, ssz_encoder::SSZEncoding};

pub struct LeanHeadField {
    pub db: Arc<Database>,
}

/// Table definition for the Lean_Head table
///
/// Value: B256
impl REDBField for LeanHeadField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, SSZEncoding<B256>> =
        TableDefinition::new("lean_head");

    const KEY: &str = "lean_head_key";

    type Value = B256;

    type ValueFieldDefinition = SSZEncoding<B256>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
