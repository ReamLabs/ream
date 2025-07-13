use std::path::Path;

use alloy_primitives::B256;
use anyhow::Ok;
use async_trait::async_trait;
use serde::Deserialize;

use super::{
    engine_trait::ExecutionApi, new_payload_request::NewPayloadRequest,
    rpc_types::get_blobs::BlobAndProofV1,
};

#[derive(Deserialize, Debug, Default)]
pub struct MockExecutionEngine {
    execution_valid: bool,
}

impl MockExecutionEngine {
    pub fn new() -> Self {
        Self {
            execution_valid: true,
        }
    }

    pub fn from_file(execution_yaml_path: &Path) -> anyhow::Result<MockExecutionEngine> {
        let file = std::fs::File::open(execution_yaml_path)?;
        Ok(serde_yaml::from_reader(file)?)
    }

    pub fn set_payload_status(&mut self, payload_status: bool) {
        self.execution_valid = payload_status;
    }
}

#[async_trait]
impl ExecutionApi for MockExecutionEngine {
    async fn verify_and_notify_new_payload(
        &self,
        _new_payload_request: NewPayloadRequest,
    ) -> anyhow::Result<bool> {
        Ok(self.execution_valid)
    }

    async fn engine_get_blobs_v1(
        &self,
        blob_version_hashes: Vec<B256>,
    ) -> anyhow::Result<Vec<Option<BlobAndProofV1>>> {
        Ok(blob_version_hashes.into_iter().map(|_| None).collect())
    }
}
