use std::sync::Arc;

use ream_consensus_lean::attestation::{AggregatedSignatureProof, SignatureKey};
use redb::{Database, Durability, TableDefinition};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

/// Wrapper for a list of aggregated signature proofs.
/// Uses VariableList for SSZ compatibility.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
pub struct AggregatedPayloadList {
    pub proofs: VariableList<AggregatedSignatureProof, U4096>,
}

impl AggregatedPayloadList {
    pub fn new() -> Self {
        Self {
            proofs: VariableList::empty(),
        }
    }

    pub fn push(&mut self, proof: AggregatedSignatureProof) -> Result<(), ssz_types::Error> {
        self.proofs.push(proof)
    }

    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.proofs.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &AggregatedSignatureProof> {
        self.proofs.iter()
    }
}

impl Default for AggregatedPayloadList {
    fn default() -> Self {
        Self::new()
    }
}

/// Table for storing aggregated signature proofs learned from blocks.
/// Key: SignatureKey (validator_id, attestation_data_root)
/// Value: AggregatedPayloadList (list of AggregateSignature proofs)
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
    pub fn append_proof(
        &self,
        key: SignatureKey,
        proof: AggregatedSignatureProof,
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
