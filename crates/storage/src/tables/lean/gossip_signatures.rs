use std::sync::Arc;

use ream_consensus_lean::attestation::SignatureKey;
use ream_post_quantum_crypto::leansig::signature::Signature;
use redb::{Database, Durability, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

/// Table for storing per-validator XMSS signatures learned from gossip.
/// Key: SignatureKey (validator_id, attestation_data_root)
/// Value: Signature
pub struct GossipSignaturesTable {
    pub db: Arc<Database>,
}

impl REDBTable for GossipSignaturesTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<SignatureKey>, SSZEncoding<Signature>> =
        TableDefinition::new("gossip_signatures");

    type Key = SignatureKey;
    type KeyTableDefinition = SSZEncoding<SignatureKey>;
    type Value = Signature;
    type ValueTableDefinition = SSZEncoding<Signature>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl GossipSignaturesTable {
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
