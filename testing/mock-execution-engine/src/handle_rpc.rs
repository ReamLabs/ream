use std::sync::{Arc, Mutex};

use actix_web::{HttpResponse, Responder, web};
use alloy_primitives::{B64, B256};
use ream_execution_rpc_types::{
    execution_payload::ExecutionPayloadV3,
    forkchoice_update::{ForkchoiceStateV1, PayloadAttributesV3},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::block_generator::ExecutionBlockGenerator;

pub type SharedExecutionBlockGenerator = Arc<Mutex<ExecutionBlockGenerator>>;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Vec<Value>,
}

pub async fn handle_rpc(
    generator: web::Data<SharedExecutionBlockGenerator>,
    request: web::Json<JsonRpcRequest>,
) -> impl Responder {
    let id = request.id.clone();
    let response = match handle_rpc_request(&generator, request.into_inner()) {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
        Err(err) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": err,
        }),
    };

    HttpResponse::Ok().json(response)
}

fn handle_rpc_request(
    generator: &SharedExecutionBlockGenerator,
    request: JsonRpcRequest,
) -> Result<Value, Value> {
    match request.method.as_str() {
        "engine_forkchoiceUpdatedV3" => {
            let forkchoice_state: ForkchoiceStateV1 = parse_param(&request.params, 0)?;
            let payload_attributes: Option<PayloadAttributesV3> = parse_param(&request.params, 1)?;
            let mut generator = lock_generator(generator)?;
            let response = generator
                .forkchoice_updated(forkchoice_state, payload_attributes)
                .map_err(invalid_params)?;
            Ok(json!({
                "payloadStatus": {
                    "status": "VALID",
                    "latestValidHash": response.payload_status.latest_valid_hash,
                    "validationError": response.payload_status.validation_error,
                },
                "payloadId": response.payload_id,
            }))
        }
        "engine_getPayloadV4" => {
            let payload_id: B64 = parse_param(&request.params, 0)?;
            let mut generator = lock_generator(generator)?;
            let payload = generator.get_payload(&payload_id).map_err(invalid_params)?;
            Ok(json!({
                "executionPayload": payload.execution_payload,
                "blockValue": payload.block_value,
                "blobsBundle": payload.blobs_bundle,
                "shouldOverrideBuilder": payload.should_override_builder,
                "executionRequests": payload.execution_requests,
            }))
        }
        "engine_newPayloadV4" => {
            let payload: ExecutionPayloadV3 = parse_param(&request.params, 0)?;
            let mut generator = lock_generator(generator)?;
            let response = generator.new_payload(payload);
            Ok(json!({
                "status": "VALID",
                "latestValidHash": response.latest_valid_hash,
                "validationError": response.validation_error,
            }))
        }
        "engine_getBlobsV1" => {
            let versioned_hashes: Vec<B256> = parse_param(&request.params, 0)?;
            let generator = lock_generator(generator)?;
            let results: Vec<Value> = versioned_hashes
                .into_iter()
                .map(|hash| match generator.get_blob_and_proof(hash) {
                    Some(blob_and_proof) => json!({
                        "blob": blob_and_proof.blob,
                        "proof": blob_and_proof.proof,
                    }),
                    None => Value::Null,
                })
                .collect();
            Ok(json!(results))
        }
        "engine_exchangeCapabilities" => Ok(json!([
            "engine_forkchoiceUpdatedV3",
            "engine_getBlobsV1",
            "engine_getPayloadV4",
            "engine_newPayloadV4",
        ])),
        _ => Err(json_rpc_error(
            -32601,
            format!("method not found: {}", request.method),
        )),
    }
}

fn parse_param<T: serde::de::DeserializeOwned>(params: &[Value], index: usize) -> Result<T, Value> {
    let value = params
        .get(index)
        .ok_or_else(|| json_rpc_error(-32602, format!("missing param {index}")))?;
    serde_json::from_value(value.clone()).map_err(invalid_params)
}

fn lock_generator(
    generator: &SharedExecutionBlockGenerator,
) -> Result<std::sync::MutexGuard<'_, ExecutionBlockGenerator>, Value> {
    generator
        .lock()
        .map_err(|err| json_rpc_error(-32603, format!("generator lock poisoned: {err}")))
}

fn invalid_params(error: impl std::fmt::Display) -> Value {
    json_rpc_error(-32602, error.to_string())
}

fn json_rpc_error(code: i64, message: impl Into<String>) -> Value {
    json!({
        "code": code,
        "message": message.into(),
    })
}
