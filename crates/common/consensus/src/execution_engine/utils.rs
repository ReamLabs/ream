use serde::{Deserialize, Serialize};

pub fn strip_prefix(s: &str) -> &str {
    if let Some(stripped) = s.strip_prefix("0x") {
        stripped
    } else {
        s
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
pub struct JsonRpcResponse<T> {
    pub result: T,
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
