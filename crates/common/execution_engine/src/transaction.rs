use alloy_primitives::{Address, Bytes, B256, U256, U64};
use alloy_rlp::{bytes, Buf, Decodable, Encodable, RlpDecodable, RlpEncodable, EMPTY_STRING_CODE};
use serde::Deserialize;

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
