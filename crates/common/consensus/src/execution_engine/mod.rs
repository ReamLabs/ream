pub mod new_payload_request;
mod rpc_types;

use alloy_primitives::{hex, Address, Bytes, B256, U256, U64};
use alloy_rlp::{bytes, Buf, Decodable, Encodable, RlpDecodable, RlpEncodable, EMPTY_STRING_CODE};
use anyhow::anyhow;
use jsonwebtoken::{encode, get_current_timestamp, EncodingKey, Header};
use new_payload_request::NewPayloadRequest;
use reqwest::Client;
use rpc_types::eth_syncing::EthSyncing;
use serde::{Deserialize, Serialize};

use crate::deneb::execution_payload::ExecutionPayload;

// Define a wrapper struct to extract "result" without cloning
#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    result: T,
}

pub struct ExecutionEngine {
    http_client: Client,
    jwt_encoding_key: EncodingKey,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    /// issued-at claim. Represented as seconds passed since UNIX_EPOCH.
    iat: u64,
    /// Optional unique identifier for the CL node.
    id: Option<String>,
    /// Optional client version for the CL node.
    clv: Option<String>,
}

impl ExecutionEngine {
    pub fn new(jwt_path: &str) -> anyhow::Result<ExecutionEngine> {
        let jwt_file = std::fs::read_to_string(jwt_path)?;
        let jwt_private_key = hex::decode(strip_prefix(jwt_file.trim_end()))?;
        Ok(ExecutionEngine {
            http_client: Client::new(),
            jwt_encoding_key: EncodingKey::from_secret(jwt_private_key.as_slice()),
        })
    }

    pub fn create_jwt_token(&self) -> anyhow::Result<String> {
        let header = Header::default();
        let claims = Claims {
            iat: get_current_timestamp(),
            id: None,
            clv: None,
        };
        encode(&header, &claims, &self.jwt_encoding_key)
            .map_err(|err| anyhow!("Could not encode jwt key {err:?}"))
    }

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

    pub async fn eth_syncing(&self) -> anyhow::Result<EthSyncing> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_syncing".to_string(),
            params: vec![],
        };
        let http_post_request = self
            .http_client
            .post("http://127.0.0.1:8551")
            .json(&request_body)
            .bearer_auth(self.create_jwt_token()?)
            .build();
        Ok(self
            .http_client
            .execute(http_post_request?)
            .await?
            .json::<JsonRpcResponse<EthSyncing>>()
            .await
            .map(|result| result.result)?)
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

pub fn strip_prefix(s: &str) -> &str {
    if let Some(stripped) = s.strip_prefix("0x") {
        stripped
    } else {
        s
    }
}

#[derive(Serialize, Deserialize)]
struct JsonRpcRequest {
    id: i32,
    jsonrpc: String,
    method: String,
    params: Vec<serde_json::Value>,
}
