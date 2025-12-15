pub mod rpc_types;
pub mod utils;

use std::path::PathBuf;

use alloy_primitives::{Address, B64, B256, Bytes, U64, hex};
use alloy_rpc_types_eth::{Block, BlockId, BlockNumberOrTag, Filter, Log, TransactionRequest};
use anyhow::anyhow;
use async_trait::async_trait;
use jsonwebtoken::{EncodingKey, Header, encode, get_current_timestamp};
use ream_consensus_beacon::{
    electra::execution_payload::ExecutionPayload,
    execution_engine::{
        engine_trait::ExecutionApi, new_payload_request::NewPayloadRequest,
        rpc_types::get_blobs::BlobAndProofV1,
    },
    execution_requests::ExecutionRequests,
};
use ream_consensus_misc::constants::beacon::{
    CONSOLIDATION_REQUEST_TYPE, DEPOSIT_REQUEST_TYPE, WITHDRAWAL_REQUEST_TYPE,
};
use reqwest::{Client, Request, Url};
use rpc_types::{
    eth_syncing::EthSyncing,
    execution_payload::ExecutionPayloadV3,
    forkchoice_update::{ForkchoiceStateV1, ForkchoiceUpdateResult, PayloadAttributesV3},
    get_payload::PayloadV4,
    payload_status::{PayloadStatus, PayloadStatusV1},
};
use serde_json::json;
use ssz::Encode;
use ssz_types::VariableList;
use utils::{Claims, JsonRpcRequest, JsonRpcResponse, blob_versioned_hashes, strip_prefix};

#[derive(Clone)]
pub struct ExecutionEngine {
    http_client: Client,
    jwt_encoding_key: EncodingKey,
    engine_api_url: Url,
}

impl ExecutionEngine {
    pub fn new(engine_api_url: Url, jwt_path: PathBuf) -> anyhow::Result<ExecutionEngine> {
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
        execution_requests_list: &[Bytes],
    ) -> bool {
        execution_payload.block_hash
            == execution_payload
                .to_execution_header(parent_beacon_block_root, execution_requests_list)
                .hash_slow()
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
            execution_requests,
        } = new_payload_request;
        let payload_status = self
            .engine_new_payload_v4(
                execution_payload.into(),
                versioned_hashes,
                parent_beacon_block_root,
                get_execution_requests_list(&execution_requests),
            )
            .await?;
        Ok(payload_status.status)
    }

    pub fn build_request(&self, rpc_request: JsonRpcRequest) -> anyhow::Result<Request> {
        Ok(self
            .http_client
            .post(self.engine_api_url.clone())
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

    pub async fn eth_block_number(&self) -> anyhow::Result<B64> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_blockNumber".to_string(),
            params: vec![],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<B64>>()
            .await?
            .to_result()
    }

    pub async fn eth_chain_id(&self) -> anyhow::Result<U64> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_chainId".to_string(),
            params: vec![],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<U64>>()
            .await?
            .to_result()
    }

    pub async fn eth_get_block_by_number(
        &self,
        block_number_or_tag: BlockNumberOrTag,
        hydrated: bool,
    ) -> anyhow::Result<Block> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_getBlockByNumber".to_string(),
            params: vec![json!(block_number_or_tag), json!(hydrated)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<Block>>()
            .await?
            .to_result()
    }

    pub async fn eth_get_block_by_hash(
        &self,
        block_hash: B256,
        hydrated: bool,
    ) -> anyhow::Result<Block> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_getBlockByHash".to_string(),
            params: vec![json!(block_hash), json!(hydrated)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<Block>>()
            .await?
            .to_result()
    }

    pub async fn eth_get_logs(&self, filter: Filter) -> anyhow::Result<Vec<Log>> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_getLogs".to_string(),
            params: vec![json!(filter)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<Vec<Log>>>()
            .await?
            .to_result()
    }

    pub async fn eth_call(
        &self,
        transaction: TransactionRequest,
        block: Option<BlockId>,
    ) -> anyhow::Result<Bytes> {
        let mut params = vec![json!(transaction)];
        if let Some(block) = block {
            params.push(json!(block));
        }

        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_call".to_string(),
            params,
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<Bytes>>()
            .await?
            .to_result()
    }

    pub async fn eth_send_raw_transaction(&self, transaction: Bytes) -> anyhow::Result<B256> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_sendRawTransaction".to_string(),
            params: vec![json!(transaction)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<B256>>()
            .await?
            .to_result()
    }

    pub async fn eth_get_code(&self, address: Address, block_id: BlockId) -> anyhow::Result<Bytes> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "eth_getCode".to_string(),
            params: vec![json!(address), json!(block_id)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<Bytes>>()
            .await?
            .to_result()
    }

    pub async fn engine_exchange_capabilities(&self) -> anyhow::Result<Vec<String>> {
        let capabilities: Vec<String> = vec![
            "engine_forkchoiceUpdatedV3".to_string(),
            "engine_getBlobsV1".to_string(),
            "engine_getPayloadV4".to_string(),
            "engine_newPayloadV4".to_string(),
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

    pub async fn engine_get_payload_v4(&self, payload_id: B64) -> anyhow::Result<PayloadV4> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "engine_getPayloadV4".to_string(),
            params: vec![json!(payload_id)],
        };

        let http_post_request = self.build_request(request_body)?;

        self.http_client
            .execute(http_post_request)
            .await?
            .json::<JsonRpcResponse<PayloadV4>>()
            .await?
            .to_result()
    }

    pub async fn engine_new_payload_v4(
        &self,
        execution_payload: ExecutionPayloadV3,
        expected_blob_versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
        execution_requests: Vec<Bytes>,
    ) -> anyhow::Result<PayloadStatusV1> {
        let request_body = JsonRpcRequest {
            id: 1,
            jsonrpc: "2.0".to_string(),
            method: "engine_newPayloadV4".to_string(),
            params: vec![
                json!(execution_payload),
                json!(expected_blob_versioned_hashes),
                json!(parent_beacon_block_root),
                json!(execution_requests),
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
}

/// Return ``True`` if and only if the version hashes computed by the blob transactions of
/// ``new_payload_request.execution_payload`` matches ``new_payload_request.versioned_hashes``.
pub fn is_valid_versioned_hashes(new_payload_request: &NewPayloadRequest) -> anyhow::Result<bool> {
    Ok(
        blob_versioned_hashes(&new_payload_request.execution_payload.transactions)?
            == new_payload_request.versioned_hashes,
    )
}

fn get_execution_requests_list(execution_requests: &ExecutionRequests) -> Vec<Bytes> {
    let mut requests_list = vec![];
    if !execution_requests.deposits.is_empty() {
        requests_list.push(Bytes::from(
            [
                vec![DEPOSIT_REQUEST_TYPE],
                execution_requests.deposits.as_ssz_bytes(),
            ]
            .concat(),
        ));
    }
    if !execution_requests.withdrawals.is_empty() {
        requests_list.push(Bytes::from(
            [
                vec![WITHDRAWAL_REQUEST_TYPE],
                execution_requests.withdrawals.as_ssz_bytes(),
            ]
            .concat(),
        ));
    }
    if !execution_requests.consolidations.is_empty() {
        requests_list.push(Bytes::from(
            [
                vec![CONSOLIDATION_REQUEST_TYPE],
                execution_requests.consolidations.as_ssz_bytes(),
            ]
            .concat(),
        ));
    }
    requests_list
}

#[async_trait]
impl ExecutionApi for ExecutionEngine {
    async fn verify_and_notify_new_payload(
        &self,
        new_payload_request: NewPayloadRequest,
    ) -> anyhow::Result<bool> {
        let execution_requests_list =
            get_execution_requests_list(&new_payload_request.execution_requests);
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
            &execution_requests_list,
        ) {
            return Ok(false);
        }

        if !is_valid_versioned_hashes(&new_payload_request)? {
            return Ok(false);
        }

        return Ok(self.notify_new_payload(new_payload_request).await? == PayloadStatus::Valid);
    }

    async fn engine_get_blobs_v1(
        &self,
        blob_version_hashes: Vec<B256>,
    ) -> anyhow::Result<Vec<Option<BlobAndProofV1>>> {
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
            .json::<JsonRpcResponse<Vec<Option<BlobAndProofV1>>>>()
            .await?
            .to_result()
    }
}
