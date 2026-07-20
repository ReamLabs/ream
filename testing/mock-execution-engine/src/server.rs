use std::{
    net::TcpListener,
    sync::{Arc, Mutex},
};

use actix_web::{App, HttpServer, dev::ServerHandle, web};
use alloy_primitives::B256;
use url::Url;

use crate::{block_generator::ExecutionBlockGenerator, handle_rpc::handle_rpc};

pub struct MockExecutionServer {
    url: Url,
    handle: ServerHandle,
    generator: Arc<Mutex<ExecutionBlockGenerator>>,
}

impl MockExecutionServer {
    pub fn start(genesis_block_hash: B256) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock EL");
        let address = listener
            .local_addr()
            .expect("failed to read mock EL address");
        let generator = Arc::new(Mutex::new(ExecutionBlockGenerator::new(genesis_block_hash)));
        let app_generator = Arc::clone(&generator);
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(Arc::clone(&app_generator)))
                .route("/", web::post().to(handle_rpc))
        })
        .listen(listener)
        .expect("failed to listen on mock EL socket")
        .run();
        let handle = server.handle();
        tokio::spawn(server);

        Self {
            url: Url::parse(&format!("http://{address}")).expect("mock EL url should parse"),
            handle,
            generator,
        }
    }

    pub fn url(&self) -> Url {
        self.url.clone()
    }

    pub fn generator(&self) -> Arc<Mutex<ExecutionBlockGenerator>> {
        Arc::clone(&self.generator)
    }

    pub async fn stop(self) {
        self.handle.stop(true).await;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use alloy_primitives::{B256, Bytes};
    use ream_execution_engine::ExecutionEngine;
    use ream_execution_rpc_types::forkchoice_update::ForkchoiceStateV1;
    use serde_json::json;

    use super::*;
    use crate::block_generator::{ensure_valid_status, test_payload_attributes};

    fn forkchoice_state(head_block_hash: B256) -> serde_json::Value {
        json!({
            "headBlockHash": head_block_hash,
            "safeBlockHash": head_block_hash,
            "finalizedBlockHash": head_block_hash,
        })
    }

    #[tokio::test]
    async fn raw_fcu_returns_payload_id() {
        let genesis_hash = B256::with_last_byte(1);
        let server = MockExecutionServer::start(genesis_hash);
        let client = reqwest::Client::new();

        let response: serde_json::Value = client
            .post(server.url())
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "engine_forkchoiceUpdatedV3",
                "params": [
                    forkchoice_state(genesis_hash),
                    test_payload_attributes(B256::with_last_byte(2)),
                ],
            }))
            .send()
            .await
            .expect("request succeeds")
            .json()
            .await
            .expect("response is json");

        server.stop().await;

        assert_eq!(response["result"]["payloadStatus"]["status"], "VALID");
        assert!(response["result"]["payloadId"].is_string());
    }

    #[tokio::test]
    async fn unknown_method_is_jsonrpc_error() {
        let server = MockExecutionServer::start(B256::with_last_byte(1));
        let client = reqwest::Client::new();

        let response: serde_json::Value = client
            .post(server.url())
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "engine_fooBar",
                "params": [],
            }))
            .send()
            .await
            .expect("request succeeds")
            .json()
            .await
            .expect("response is json");

        server.stop().await;

        assert_eq!(response["error"]["code"], -32601);
        assert!(response.get("result").is_none());
    }

    #[tokio::test]
    async fn ream_execution_engine_drives_mock_end_to_end() {
        let genesis_hash = B256::with_last_byte(1);
        let parent_beacon_block_root = B256::with_last_byte(2);
        let server = MockExecutionServer::start(genesis_hash);
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let jwt_path = tempdir.path().join("jwt.hex");
        fs::write(
            &jwt_path,
            "4242424242424242424242424242424242424242424242424242424242424242",
        )
        .expect("jwt should be written");
        let engine =
            ExecutionEngine::new(server.url(), jwt_path).expect("execution engine should build");
        let state = ForkchoiceStateV1 {
            head_block_hash: genesis_hash,
            safe_block_hash: genesis_hash,
            finalized_block_hash: genesis_hash,
        };
        let attrs = test_payload_attributes(parent_beacon_block_root);

        let forkchoice = engine
            .engine_forkchoice_updated_v3(state, Some(attrs))
            .await
            .expect("forkchoice update should succeed");
        ensure_valid_status(&forkchoice.payload_status).expect("forkchoice status should be valid");

        let payload = engine
            .engine_get_payload_v4(forkchoice.payload_id.expect("payload id should exist"))
            .await
            .expect("get payload should succeed");
        assert_eq!(payload.execution_payload.parent_hash, genesis_hash);

        let status = engine
            .engine_new_payload_v4(
                payload.execution_payload,
                Vec::new(),
                parent_beacon_block_root,
                Vec::<Bytes>::new(),
            )
            .await
            .expect("new payload should succeed");
        ensure_valid_status(&status).expect("new payload status should be valid");

        server.stop().await;
    }
}
