use std::path::Path;

use async_trait::async_trait;
use serde::Deserialize;

use super::{engine_trait::ExecutionApi, new_payload_request::NewPayloadRequest};

#[derive(Deserialize, Debug)]
pub struct MockExecutionEngine {
    execution_valid: bool,
}

impl MockExecutionEngine {
    pub fn new(execution_yaml_path: &Path) -> anyhow::Result<MockExecutionEngine> {
        let file = std::fs::File::open(execution_yaml_path)?;
        Ok(serde_yaml::from_reader(file)?)
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
}
