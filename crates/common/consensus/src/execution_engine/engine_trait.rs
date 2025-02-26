use async_trait::async_trait;

use super::new_payload_request::NewPayloadRequest;

#[async_trait]
pub trait ExecutionApi {
    /// Return ``True`` if and only if ``new_payload_request`` is valid with respect to
    /// ``self.execution_state``.
    async fn verify_and_notify_new_payload(
        &self,
        new_payload_request: NewPayloadRequest,
    ) -> anyhow::Result<bool>;
}
