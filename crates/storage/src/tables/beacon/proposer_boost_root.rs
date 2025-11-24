use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, TableDefinition};

use crate::tables::{field::REDBField, ssz_encoder::SSZEncoding};

pub struct ProposerBoostRootField {
    pub db: Arc<Database>,
}

/// Table definition for the Proposer_Boost_Root table
///
/// Value: Root
impl REDBField for ProposerBoostRootField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, SSZEncoding<B256>> =
        TableDefinition::new("beacon_proposer_boost_root");

    const KEY: &str = "proposer_boost_root_key";

    type Value = B256;

    type ValueFieldDefinition = SSZEncoding<B256>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
