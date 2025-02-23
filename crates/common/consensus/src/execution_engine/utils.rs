use anyhow::bail;
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
