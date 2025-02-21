pub mod new_payload_request;
mod rpc_types;
pub mod transaction;
pub mod utils;

use alloy_primitives::{hex, B256};
use alloy_rlp::Decodable;
use anyhow::anyhow;
use jsonwebtoken::{encode, get_current_timestamp, EncodingKey, Header};
use new_payload_request::NewPayloadRequest;
use reqwest::Client;
use rpc_types::eth_syncing::EthSyncing;
use transaction::{BlobTransaction, TransactionType};
use utils::{strip_prefix, Claims, JsonRpcRequest, JsonRpcResponse};

use crate::deneb::execution_payload::ExecutionPayload;

pub struct ExecutionEngine {
    http_client: Client,
    jwt_encoding_key: EncodingKey,
    engine_api_url: String,
}

impl ExecutionEngine {
    pub fn new(jwt_path: &str, engine_api_url: String) -> anyhow::Result<ExecutionEngine> {
        let jwt_file = std::fs::read_to_string(jwt_path)?;
        let jwt_private_key = hex::decode(strip_prefix(jwt_file.trim_end()))?;
        Ok(ExecutionEngine {
            http_client: Client::new(),
            jwt_encoding_key: EncodingKey::from_secret(jwt_private_key.as_slice()),
            engine_api_url,
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
            .post(&self.engine_api_url)
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
