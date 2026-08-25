use std::sync::Arc;

use ream_consensus_lean::attestation::SignatureKey;
#[cfg(feature = "devnet5")]
use ream_consensus_lean::attestation::SingleMessageAggregate;
use redb::{Database, Durability, TableDefinition};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

/// Wrapper for a list of aggregated signature proofs.
/// Uses VariableList for SSZ compatibility.
#[cfg(feature = "devnet5")]
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
pub struct AggregatedPayloadList {
    pub proofs: VariableList<SingleMessageAggregate, U4096>,
}

#[cfg(feature = "devnet5")]
impl AggregatedPayloadList {
    pub fn new() -> Self {
        Self {
            proofs: VariableList::empty(),
        }
    }
    pub fn push(&mut self, proof: SingleMessageAggregate) -> Result<(), ssz_types::Error> {
        self.proofs.push(proof)
    }
    pub fn iter(&self) -> impl Iterator<Item = &SingleMessageAggregate> {
        self.proofs.iter()
    }
}

impl Default for AggregatedPayloadList {
    fn default() -> Self {
        Self::new()
    }
}

impl AggregatedPayloadList {
    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.proofs.len()
    }
}

/// Table for storing aggregated signature proofs learned from blocks.
pub struct AggregatedPayloadsTable {
    pub db: Arc<Database>,
}

impl REDBTable for AggregatedPayloadsTable {
    const TABLE_DEFINITION: TableDefinition<
        '_,
        SSZEncoding<SignatureKey>,
        SSZEncoding<AggregatedPayloadList>,
    > = TableDefinition::new("aggregated_payloads");

    type Key = SignatureKey;
    type KeyTableDefinition = SSZEncoding<SignatureKey>;
    type Value = AggregatedPayloadList;
    type ValueTableDefinition = SSZEncoding<AggregatedPayloadList>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl AggregatedPayloadsTable {
    /// Append a proof to the list for the given key.
    #[cfg(feature = "devnet5")]
    pub fn append_proof(
        &self,
        key: SignatureKey,
        proof: SingleMessageAggregate,
    ) -> Result<(), StoreError> {
        let mut list = self.get(key.clone())?.unwrap_or_default();
        list.push(proof)
            .map_err(|err| StoreError::DecodeError(format!("VariableList error: {err:?}")))?;
        self.insert(key, list)
    }

    /// Clear all payloads (useful for pruning old data).
    pub fn clear(&self) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
        while table.pop_first()?.is_some() {}
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
