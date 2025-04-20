use std::sync::Arc;

use ream_consensus::execution_engine::rpc_types::get_blobs::BlobsAndProofV1;
use ream_fork_choice::blob_sidecar::BlobIdentifier;
use redb::{Database, Durability, TableDefinition};

use super::{SSZEncoding, Table};
use crate::errors::StoreError;

/// Table definition for the Blobs And Proofs table
///
/// Key: blobs_and_proofs
/// Value: BlobsAndProofV1
pub const BLOBS_AND_PROOFS_TABLE: TableDefinition<
    SSZEncoding<BlobIdentifier>,
    SSZEncoding<BlobsAndProofV1>,
> = TableDefinition::new("blobs_and_proofs");

pub struct BlobsAndProofsTable {
    pub db: Arc<Database>,
}

impl Table for BlobsAndProofsTable {
    type Key = BlobIdentifier;

    type Value = BlobsAndProofV1;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(BLOBS_AND_PROOFS_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(BLOBS_AND_PROOFS_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
