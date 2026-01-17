use std::sync::Arc;

use ream_consensus_lean::attestation::SignatureKey;
use ream_post_quantum_crypto::lean_multisig::aggregate::AggregateSignature;
use redb::{Database, Durability, TableDefinition};
use ssz::{Decode, Encode};
use ssz_types::{BitList, VariableList, typenum::U4096};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

/// Aggregated signature proof with participant tracking.
/// Mirrors Python spec's AggregatedSignatureProof.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AggregatedSignatureProof {
    /// Which validators are covered by this proof
    pub participants: BitList<U4096>,
    /// The aggregated signature proof
    pub proof: AggregateSignature,
}

impl Encode for AggregatedSignatureProof {
    fn is_ssz_fixed_len() -> bool {
        false
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        // Variable-length container: encode each field with offset
        let mut encoder = ssz::SszEncoder::container(buf, 2);
        encoder.append(&self.participants);
        encoder.append(&self.proof);
        encoder.finalize();
    }

    fn ssz_bytes_len(&self) -> usize {
        // 2 offsets (4 bytes each) + variable data
        8 + self.participants.ssz_bytes_len() + self.proof.ssz_bytes_len()
    }
}

impl Decode for AggregatedSignatureProof {
    fn is_ssz_fixed_len() -> bool {
        false
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, ssz::DecodeError> {
        let mut builder = ssz::SszDecoderBuilder::new(bytes);
        builder.register_type::<BitList<U4096>>()?;
        builder.register_type::<AggregateSignature>()?;
        let mut decoder = builder.build()?;

        Ok(Self {
            participants: decoder.decode_next()?,
            proof: decoder.decode_next()?,
        })
    }
}

impl AggregatedSignatureProof {
    pub fn new(participants: BitList<U4096>, proof: AggregateSignature) -> Self {
        Self {
            participants,
            proof,
        }
    }

    /// Get the validator IDs covered by this proof
    pub fn to_validator_indices(&self) -> Vec<u64> {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, bit)| *bit)
            .map(|(index, _)| index as u64)
            .collect()
    }
}

/// Wrapper for a list of aggregated signature proofs.
/// Uses VariableList for SSZ compatibility.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AggregatedPayloadList {
    pub proofs: VariableList<AggregatedSignatureProof, U4096>,
}

impl Encode for AggregatedPayloadList {
    fn is_ssz_fixed_len() -> bool {
        false
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        self.proofs.ssz_append(buf);
    }

    fn ssz_bytes_len(&self) -> usize {
        self.proofs.ssz_bytes_len()
    }
}

impl Decode for AggregatedPayloadList {
    fn is_ssz_fixed_len() -> bool {
        false
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, ssz::DecodeError> {
        Ok(Self {
            proofs: VariableList::from_ssz_bytes(bytes)?,
        })
    }
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
    /// Get payloads by key.
    pub fn get_payloads(
        &self,
        key: &SignatureKey,
    ) -> Result<Option<AggregatedPayloadList>, StoreError> {
        self.get(key.clone())
    }

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

    /// Insert or replace the full payload list for a key.
    pub fn insert_payloads(
        &self,
        key: SignatureKey,
        payloads: AggregatedPayloadList,
    ) -> Result<(), StoreError> {
        self.insert(key, payloads)
    }

    /// Check if payloads exist for the given key.
    pub fn contains(&self, key: &SignatureKey) -> bool {
        matches!(self.get(key.clone()), Ok(Some(_)))
    }

    /// Remove payloads by key.
    pub fn remove_payloads(
        &self,
        key: &SignatureKey,
    ) -> Result<Option<AggregatedPayloadList>, StoreError> {
        self.remove(key.clone())
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
