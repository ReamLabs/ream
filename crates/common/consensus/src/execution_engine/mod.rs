pub mod new_payload_request;
mod rpc_types;
pub mod transaction;
pub mod utils;

use alloy_primitives::{hex, B256, B64};
use alloy_rlp::Decodable;
use anyhow::anyhow;
use jsonwebtoken::{encode, get_current_timestamp, EncodingKey, Header};
use new_payload_request::NewPayloadRequest;
use reqwest::{Client, Request};
use rpc_types::{
    eth_syncing::EthSyncing,
    execution_payload::ExecutionPayloadV3,
    forkchoice_update::{ForkchoiceStateV1, ForkchoiceUpdateResult, PayloadAttributesV3},
    get_blobs::BlobsAndProofV1,
    get_payload::PayloadV3,
    payload_status::{PayloadStatus, PayloadStatusV1},
};
use serde_json::json;
use ssz_types::VariableList;
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
        execution_payload: &ExecutionPayload,
        parent_beacon_block_root: B256,
    ) -> bool {
        execution_payload.block_hash == execution_payload.header_hash(parent_beacon_block_root)
    }

    pub fn blob_versioned_hashes(
        &self,
        execution_payload: &ExecutionPayload,
    ) -> anyhow::Result<Vec<B256>> {
        let mut blob_versioned_hashes = vec![];
        for transaction in execution_payload.transactions.iter() {
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

    /// Return ``True`` if and only if the version hashes computed by the blob transactions of
    /// ``new_payload_request.execution_payload`` matches ``new_payload_request.versioned_hashes``.
    pub fn is_valid_versioned_hashes(
        &self,
        new_payload_request: &NewPayloadRequest,
    ) -> anyhow::Result<bool> {
        Ok(
            self.blob_versioned_hashes(&new_payload_request.execution_payload)?
                == new_payload_request.versioned_hashes,
        )
    }

    /// Return ``PayloadStatus`` of execution payload``.
    pub async fn notify_new_payload(
        &self,
        new_payload_request: NewPayloadRequest,
    ) -> anyhow::Result<PayloadStatus> {
        let NewPayloadRequest {
            execution_payload,
            versioned_hashes,
            parent_beacon_block_root,
        } = new_payload_request;
        let payload_status = self
            .engine_new_payload_v3(
                execution_payload.into(),
                versioned_hashes,
                parent_beacon_block_root,
            )
            .await?;
        Ok(payload_status.status)
    }

    /// Return ``True`` if and only if ``new_payload_request`` is valid with respect to
    /// ``self.execution_state``.
    pub async fn verify_and_notify_new_payload(
        &self,
        new_payload_request: NewPayloadRequest,
    ) -> anyhow::Result<bool> {
        if new_payload_request
            .execution_payload
            .transactions
            .contains(&VariableList::empty())
        {
            return Ok(false);
        }

        if !self.is_valid_block_hash(
            &new_payload_request.execution_payload,
            new_payload_request.parent_beacon_block_root,
        ) {
            return Ok(false);
        }

        if !self.is_valid_versioned_hashes(&new_payload_request)? {
            return Ok(false);
        }

        return Ok(self.notify_new_payload(new_payload_request).await? == PayloadStatus::Valid);
    }

    pub fn build_request(&self, rpc_request: JsonRpcRequest) -> anyhow::Result<Request> {
        Ok(self
            .http_client
            .post(&self.engine_api_url)
            .json(&rpc_request)
            .bearer_auth(self.create_jwt_token()?)
            .build()?)
    }

    pub async fn eth_syncing(&self) -> anyhow::Result<EthSyncing> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_syncing".to_string(),
            params: vec![],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<EthSyncing>>()
            .await?
            .to_result()
    }

    pub async fn engine_exchange_capabilities(&self) -> anyhow::Result<Vec<String>> {
        let capabilities: Vec<String> = vec![
            "engine_forkchoiceUpdatedV3".to_string(),
            "engine_getBlobsV1".to_string(),
            "engine_getPayloadV3".to_string(),
            "engine_newPayloadV3".to_string(),
        ];
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "engine_exchangeCapabilities".to_string(),
            params: vec![json!(capabilities)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<Vec<String>>>()
            .await?
            .to_result()
    }

    pub async fn engine_get_payload_v3(&self, payload_id: B64) -> anyhow::Result<PayloadV3> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "engine_getPayloadV3".to_string(),
            params: vec![json!(payload_id)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<PayloadV3>>()
            .await?
            .to_result()
    }

    pub async fn engine_new_payload_v3(
        &self,
        execution_payload: ExecutionPayloadV3,
        expected_blob_versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> anyhow::Result<PayloadStatusV1> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "engine_newPayloadV3".to_string(),
            params: vec![
                json!(execution_payload),
                json!(expected_blob_versioned_hashes),
                json!(parent_beacon_block_root),
            ],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<PayloadStatusV1>>()
            .await?
            .to_result()
    }

    pub async fn engine_forkchoice_updated_v3(
        &self,
        forkchoice_state: ForkchoiceStateV1,
        payload_attributes: Option<PayloadAttributesV3>,
    ) -> anyhow::Result<ForkchoiceUpdateResult> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "engine_forkchoiceUpdatedV3".to_string(),
            params: vec![json!(forkchoice_state), json!(payload_attributes)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<ForkchoiceUpdateResult>>()
            .await?
            .to_result()
    }

    pub async fn engine_get_blobs_v1(
        &self,
        blob_version_hashes: Vec<B256>,
    ) -> anyhow::Result<Vec<Option<BlobsAndProofV1>>> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "engine_getBlobsV1".to_string(),
            params: vec![json!(blob_version_hashes)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<Vec<Option<BlobsAndProofV1>>>>()
            .await?
            .to_result()
    }
}
