use alloy_primitives::B256;
use alloy_rlp::Decodable;
use anyhow::{anyhow, bail};
use ream_consensus::{
    electra::execution_payload::Transactions,
    execution_engine::rpc_types::transaction::{BlobTransaction, TransactionType},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn strip_prefix(string: &str) -> &str {
    if let Some(stripped) = string.strip_prefix("0x") {
        stripped
    } else {
        string
    }
}

#[derive(Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub id: i32,
    pub jsonrpc: String,
    pub method: String,
    pub params: Vec<serde_json::Value>,
}

// Define a wrapper struct to extract "result" without cloning
#[derive(Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse<T> {
    Result { result: T },
    Error(Value),
}

impl<T> JsonRpcResponse<T> {
    pub fn to_result(self) -> anyhow::Result<T> {
        match self {
            JsonRpcResponse::Result { result } => Ok(result),
            JsonRpcResponse::Error(err) => bail!("Failed to desirilze json {err:?}"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Claims {
    /// issued-at claim. Represented as seconds passed since UNIX_EPOCH.
    pub iat: u64,
    /// Optional unique identifier for the CL node.
    pub id: Option<String>,
    /// Optional client version for the CL node.
    pub clv: Option<String>,
}

pub fn blob_versioned_hashes(transactions: &Transactions) -> anyhow::Result<Vec<B256>> {
    let mut blob_versioned_hashes = vec![];
    for transaction in transactions.iter() {
        if TransactionType::try_from(&transaction[..])
            .map_err(|err| anyhow!("Failed to detect transaction type: {err:?}"))?
            == TransactionType::BlobTransaction
        {
            let blob_transaction = BlobTransaction::decode(&mut &transaction[1..])?;
            blob_versioned_hashes.extend(blob_transaction.blob_versioned_hashes);
        }
    }
    Ok(blob_versioned_hashes)
}
