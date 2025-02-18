pub mod new_payload_request;

use alloy_primitives::{Address, Bytes, B256, U256, U64};
use alloy_rlp::{bytes, Buf, Decodable, Encodable, RlpDecodable, RlpEncodable, EMPTY_STRING_CODE};
use anyhow::anyhow;
use new_payload_request::NewPayloadRequest;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::deneb::execution_payload::ExecutionPayload;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ExecutionEngine {}

impl ExecutionEngine {
    /// Return ``True`` if and only if ``execution_payload.block_hash`` is computed correctly.
    pub fn is_valid_block_hash(
        &self,
        execution_payload: ExecutionPayload,
        parent_beacon_block_root: B256,
    ) -> bool {
        execution_payload.block_hash == execution_payload.header_hash(parent_beacon_block_root)
    }

    /// Return ``True`` if and only if the version hashes computed by the blob transactions of
    /// ``new_payload_request.execution_payload`` matches ``new_payload_request.versioned_hashes``.
    pub fn is_valid_versioned_hashes(
        &self,
        new_payload_request: NewPayloadRequest,
    ) -> anyhow::Result<bool> {
        let mut blob_versioned_hashes = vec![];
        for transaction in new_payload_request.execution_payload.transactions {
            if TransactionType::try_from(&transaction[..])
                .map_err(|err| anyhow!("Failed to detect transaction type: {err:?}"))?
                == TransactionType::BlobTransaction
            {
                let blob_transaction = BlobTransaction::decode(&mut &transaction[1..])?;
                blob_versioned_hashes.extend(blob_transaction.blob_versioned_hashes);
            }
        }

        Ok(blob_versioned_hashes == new_payload_request.versioned_hashes)
    }
}

#[derive(Default, Eq, Debug, Clone, PartialEq)]
pub enum ToAddress {
    #[default]
    Empty,
    Exists(Address),
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct AccessList {
    pub list: Vec<AccessListItem>,
}

impl Decodable for AccessList {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let list: Vec<AccessListItem> = Decodable::decode(buf)?;
        Ok(Self { list })
    }
}

impl Encodable for AccessList {
    fn encode(&self, out: &mut dyn bytes::BufMut) {
        self.list.encode(out);
    }
}

impl Encodable for ToAddress {
    fn encode(&self, out: &mut dyn bytes::BufMut) {
        match self {
            ToAddress::Empty => {
                out.put_u8(EMPTY_STRING_CODE);
            }
            ToAddress::Exists(addr) => {
                addr.0.encode(out);
            }
        }
    }
}

impl Decodable for ToAddress {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        if let Some(&first) = buf.first() {
            if first == EMPTY_STRING_CODE {
                buf.advance(1);
                Ok(ToAddress::Empty)
            } else {
                Ok(ToAddress::Exists(Address::decode(buf)?))
            }
        } else {
            Err(alloy_rlp::Error::InputTooShort)
        }
    }
}

#[derive(Debug, PartialEq, Clone, Eq, Deserialize, RlpDecodable, RlpEncodable)]
#[serde(rename_all = "camelCase")]
pub struct AccessListItem {
    pub address: Address,
    pub storage_keys: Vec<B256>,
}

#[derive(Eq, Debug, Clone, PartialEq, RlpDecodable, RlpEncodable)]
pub struct BlobTransaction {
    pub chain_id: U256,
    pub nonce: U256,
    pub max_priority_fee_per_gas: U256,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub to: ToAddress,
    pub value: U256,
    pub data: Bytes,
    pub access_list: AccessList,
    pub max_fee_per_blob_gas: U256,
    pub blob_versioned_hashes: Vec<B256>,
    pub y_parity: U64,
    pub r: U256,
    pub s: U256,
}

#[derive(Debug, PartialEq)]
pub enum TransactionType {
    BlobTransaction,
    LegacyTransaction,
    FeeMarketTransaction,
    AccessListTransaction,
}

#[derive(Debug)]
pub enum TransactionTypeError {
    InvalidType(u8),
    EmptyTransaction,
}

impl TryFrom<&[u8]> for TransactionType {
    type Error = TransactionTypeError;

    fn try_from(transaction: &[u8]) -> Result<Self, TransactionTypeError> {
        let first_byte = transaction
            .first()
            .ok_or(TransactionTypeError::EmptyTransaction)?;

        match first_byte {
            3 => Ok(TransactionType::BlobTransaction),
            2 => Ok(TransactionType::FeeMarketTransaction),
            1 => Ok(TransactionType::AccessListTransaction),
            _ => Ok(TransactionType::LegacyTransaction),
        }
    }
}
